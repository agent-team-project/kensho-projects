#!/usr/bin/env python3
"""Focused tests for the EL-364 Rust coverage gate."""

from __future__ import annotations

import contextlib
import importlib.util
import io
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().with_name("check_rust_coverage.py")
SPEC = importlib.util.spec_from_file_location("check_rust_coverage", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
coverage = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = coverage
SPEC.loader.exec_module(coverage)


class RustCoverageGateTest(unittest.TestCase):
    def test_summarize_scopes_to_functions_and_eval(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            lcov = root / "lcov.info"
            lcov.write_text(
                "\n".join(
                    [
                        f"SF:{root}/xlite-core/src/functions/math/sum.rs",
                        "DA:1,1",
                        "DA:2,0",
                        "end_of_record",
                        "SF:xlite-core/src/eval.rs",
                        "DA:10,2",
                        "DA:11,0",
                        "DA:12,5",
                        "end_of_record",
                        "SF:xlite-core/src/model.rs",
                        "DA:1,0",
                        "end_of_record",
                    ]
                ),
                encoding="utf-8",
            )

            summary = coverage.summarize_lcov(
                lcov,
                coverage.DEFAULT_INCLUDES,
                root=root,
            )

        self.assertEqual(summary.covered_lines, 3)
        self.assertEqual(summary.executable_lines, 5)
        self.assertAlmostEqual(summary.percent, 60.0)
        self.assertEqual(
            [file.path for file in summary.files],
            [
                "xlite-core/src/eval.rs",
                "xlite-core/src/functions/math/sum.rs",
            ],
        )

    def test_duplicate_line_records_are_combined(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            lcov = root / "lcov.info"
            lcov.write_text(
                "\n".join(
                    [
                        "SF:xlite-core/src/eval.rs",
                        "DA:10,0",
                        "DA:10,3",
                        "DA:11,0",
                        "end_of_record",
                    ]
                ),
                encoding="utf-8",
            )

            summary = coverage.summarize_lcov(
                lcov,
                ["xlite-core/src/eval.rs"],
                root=root,
            )

        self.assertEqual(summary.covered_lines, 1)
        self.assertEqual(summary.executable_lines, 2)

    def test_inline_cfg_test_modules_are_excluded(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "xlite-core" / "src" / "eval.rs"
            source.parent.mkdir(parents=True)
            source.write_text(
                "\n".join(
                    [
                        "pub fn before() {}",
                        "",
                        "#[cfg(test)]",
                        "mod tests {",
                        "    #[test]",
                        '    fn helper() { assert_eq!("{", "{"); }',
                        "}",
                        "",
                        "pub fn after() {}",
                    ]
                ),
                encoding="utf-8",
            )
            lcov = root / "lcov.info"
            lcov.write_text(
                "\n".join(
                    [
                        "SF:xlite-core/src/eval.rs",
                        "DA:1,1",
                        "DA:3,0",
                        "DA:4,0",
                        "DA:6,0",
                        "DA:7,0",
                        "DA:9,0",
                        "end_of_record",
                    ]
                ),
                encoding="utf-8",
            )

            summary = coverage.summarize_lcov(
                lcov,
                ["xlite-core/src/eval.rs"],
                root=root,
            )

        self.assertEqual(summary.covered_lines, 1)
        self.assertEqual(summary.executable_lines, 2)
        self.assertAlmostEqual(summary.percent, 50.0)

    def test_main_passes_at_exact_threshold(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            lcov = Path(directory) / "lcov.info"
            lcov.write_text(
                "\n".join(
                    [
                        "SF:xlite-core/src/eval.rs",
                        "DA:1,1",
                        "DA:2,0",
                        "end_of_record",
                    ]
                ),
                encoding="utf-8",
            )
            stdout = io.StringIO()
            stderr = io.StringIO()
            with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                result = coverage.main(
                    [
                        "--lcov",
                        str(lcov),
                        "--threshold",
                        "50",
                        "--include",
                        "xlite-core/src/eval.rs",
                    ]
                )

        self.assertEqual(result, 0)
        self.assertIn("coverage gate passed", stdout.getvalue())
        self.assertEqual(stderr.getvalue(), "")

    def test_main_fails_below_threshold(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            lcov = Path(directory) / "lcov.info"
            lcov.write_text(
                "\n".join(
                    [
                        "SF:xlite-core/src/eval.rs",
                        "DA:1,1",
                        "DA:2,0",
                        "end_of_record",
                    ]
                ),
                encoding="utf-8",
            )
            stdout = io.StringIO()
            stderr = io.StringIO()
            with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                result = coverage.main(
                    [
                        "--lcov",
                        str(lcov),
                        "--threshold",
                        "90",
                        "--include",
                        "xlite-core/src/eval.rs",
                    ]
                )

        self.assertEqual(result, 1)
        self.assertIn("coverage gate failed", stderr.getvalue())

    def test_main_fails_when_no_lines_match_scope(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            lcov = Path(directory) / "lcov.info"
            lcov.write_text(
                "\n".join(
                    [
                        "SF:xlite-core/src/model.rs",
                        "DA:1,1",
                        "end_of_record",
                    ]
                ),
                encoding="utf-8",
            )
            stdout = io.StringIO()
            stderr = io.StringIO()
            with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                result = coverage.main(
                    [
                        "--lcov",
                        str(lcov),
                        "--threshold",
                        "90",
                        "--include",
                        "xlite-core/src/eval.rs",
                    ]
                )

        self.assertEqual(result, 1)
        self.assertIn("no executable lines matched", stderr.getvalue())


if __name__ == "__main__":
    unittest.main()
