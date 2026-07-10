#!/usr/bin/env python3
"""Render the Excel Lite Kensho retrospective into a shareable PDF."""

from __future__ import annotations

import argparse
import html
import re
from pathlib import Path

from reportlab.lib import colors
from reportlab.lib.enums import TA_CENTER, TA_RIGHT
from reportlab.lib.pagesizes import A4
from reportlab.lib.styles import ParagraphStyle, getSampleStyleSheet
from reportlab.lib.units import mm
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.ttfonts import TTFont
from reportlab.platypus import (
    BaseDocTemplate,
    Flowable,
    Frame,
    Image,
    ListFlowable,
    ListItem,
    NextPageTemplate,
    PageBreak,
    PageTemplate,
    Paragraph,
    Spacer,
    Table,
    TableStyle,
)
from reportlab.platypus.tableofcontents import TableOfContents


PAGE_WIDTH, PAGE_HEIGHT = A4
MARGIN_X = 17 * mm
MARGIN_TOP = 18 * mm
MARGIN_BOTTOM = 17 * mm
CONTENT_WIDTH = PAGE_WIDTH - 2 * MARGIN_X

INK = colors.HexColor("#152822")
DEEP_GREEN = colors.HexColor("#145C43")
GREEN = colors.HexColor("#1F7A55")
MINT = colors.HexColor("#DCEFE5")
PALE_GREEN = colors.HexColor("#F1F8F4")
PALE_AMBER = colors.HexColor("#FFF3DE")
AMBER = colors.HexColor("#B76513")
GRAY_900 = colors.HexColor("#28332F")
GRAY_700 = colors.HexColor("#53615C")
GRAY_500 = colors.HexColor("#7D8A85")
GRAY_300 = colors.HexColor("#C9D1CE")
GRAY_200 = colors.HexColor("#E1E6E4")
GRAY_100 = colors.HexColor("#F5F7F6")
WHITE = colors.white

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SOURCE = ROOT / "docs/release/kensho-evaluation-report.md"
DEFAULT_OUTPUT = ROOT / "docs/release/kensho-excel-lite-retrospective.pdf"
SCREENSHOT_DIR = ROOT / "docs/verification/el-final-ui-boot-product-pass"


def register_fonts() -> tuple[str, str, str]:
    font_dir = Path("/System/Library/Fonts/Supplemental")
    files = (font_dir / "Arial.ttf", font_dir / "Arial Bold.ttf", font_dir / "Arial Italic.ttf")
    if all(path.exists() for path in files):
        pdfmetrics.registerFont(TTFont("ReportSans", str(files[0])))
        pdfmetrics.registerFont(TTFont("ReportSans-Bold", str(files[1])))
        pdfmetrics.registerFont(TTFont("ReportSans-Italic", str(files[2])))
        pdfmetrics.registerFontFamily(
            "ReportSans",
            normal="ReportSans",
            bold="ReportSans-Bold",
            italic="ReportSans-Italic",
            boldItalic="ReportSans-Bold",
        )
        return "ReportSans", "ReportSans-Bold", "ReportSans-Italic"
    return "Helvetica", "Helvetica-Bold", "Helvetica-Oblique"


FONT, FONT_BOLD, FONT_ITALIC = register_fonts()


def build_styles() -> dict[str, ParagraphStyle]:
    base = getSampleStyleSheet()
    return {
        "body": ParagraphStyle(
            "Body",
            parent=base["BodyText"],
            fontName=FONT,
            fontSize=9.25,
            leading=13.1,
            textColor=GRAY_900,
            spaceAfter=7,
            splitLongWords=True,
        ),
        "h2": ParagraphStyle(
            "H2",
            parent=base["Heading1"],
            fontName=FONT_BOLD,
            fontSize=17,
            leading=20,
            textColor=DEEP_GREEN,
            spaceBefore=10,
            spaceAfter=8,
            keepWithNext=True,
        ),
        "h3": ParagraphStyle(
            "H3",
            parent=base["Heading2"],
            fontName=FONT_BOLD,
            fontSize=11.6,
            leading=14,
            textColor=INK,
            spaceBefore=8,
            spaceAfter=5,
            keepWithNext=True,
        ),
        "table_header": ParagraphStyle(
            "TableHeader",
            parent=base["BodyText"],
            fontName=FONT_BOLD,
            fontSize=7.4,
            leading=9.1,
            textColor=WHITE,
            splitLongWords=True,
        ),
        "table_cell": ParagraphStyle(
            "TableCell",
            parent=base["BodyText"],
            fontName=FONT,
            fontSize=7.25,
            leading=9.4,
            textColor=GRAY_900,
            splitLongWords=True,
        ),
        "table_cell_right": ParagraphStyle(
            "TableCellRight",
            parent=base["BodyText"],
            fontName=FONT,
            fontSize=7.25,
            leading=9.4,
            textColor=GRAY_900,
            alignment=TA_RIGHT,
            splitLongWords=True,
        ),
        "list": ParagraphStyle(
            "List",
            parent=base["BodyText"],
            fontName=FONT,
            fontSize=9,
            leading=12.7,
            textColor=GRAY_900,
            spaceAfter=3,
            splitLongWords=True,
        ),
        "caption": ParagraphStyle(
            "Caption",
            parent=base["BodyText"],
            fontName=FONT,
            fontSize=7.1,
            leading=9.1,
            textColor=GRAY_700,
            alignment=TA_CENTER,
        ),
        "cover_kicker": ParagraphStyle(
            "CoverKicker",
            parent=base["BodyText"],
            fontName=FONT_BOLD,
            fontSize=10,
            leading=12,
            textColor=colors.HexColor("#72D2A6"),
            spaceAfter=5,
        ),
        "cover_title": ParagraphStyle(
            "CoverTitle",
            parent=base["Title"],
            fontName=FONT_BOLD,
            fontSize=31,
            leading=34,
            textColor=WHITE,
            spaceAfter=10,
        ),
        "cover_deck": ParagraphStyle(
            "CoverDeck",
            parent=base["BodyText"],
            fontName=FONT,
            fontSize=12,
            leading=17,
            textColor=colors.HexColor("#D9E5E0"),
            spaceAfter=14,
        ),
        "cover_metric_value": ParagraphStyle(
            "CoverMetricValue",
            parent=base["BodyText"],
            fontName=FONT_BOLD,
            fontSize=18,
            leading=20,
            textColor=WHITE,
            alignment=TA_CENTER,
        ),
        "cover_metric_label": ParagraphStyle(
            "CoverMetricLabel",
            parent=base["BodyText"],
            fontName=FONT,
            fontSize=7.2,
            leading=9,
            textColor=colors.HexColor("#BDD0C8"),
            alignment=TA_CENTER,
        ),
        "cover_meta": ParagraphStyle(
            "CoverMeta",
            parent=base["BodyText"],
            fontName=FONT,
            fontSize=8.2,
            leading=11,
            textColor=colors.HexColor("#B8C9C2"),
        ),
        "status_label": ParagraphStyle(
            "StatusLabel",
            parent=base["BodyText"],
            fontName=FONT,
            fontSize=7.2,
            leading=9,
            textColor=GRAY_700,
        ),
        "status_value": ParagraphStyle(
            "StatusValue",
            parent=base["BodyText"],
            fontName=FONT_BOLD,
            fontSize=10.2,
            leading=12,
            textColor=INK,
        ),
        "toc_title": ParagraphStyle(
            "TOCTitle",
            parent=base["Heading1"],
            fontName=FONT_BOLD,
            fontSize=22,
            leading=26,
            textColor=DEEP_GREEN,
            spaceAfter=14,
        ),
    }


STYLES = build_styles()


class Rule(Flowable):
    def __init__(self, color=GRAY_200, thickness: float = 0.8, space: float = 7):
        super().__init__()
        self.color = color
        self.thickness = thickness
        self.width = CONTENT_WIDTH
        self.height = space

    def draw(self) -> None:
        self.canv.setStrokeColor(self.color)
        self.canv.setLineWidth(self.thickness)
        self.canv.line(0, self.height / 2, self.width, self.height / 2)


class RetrospectiveDocTemplate(BaseDocTemplate):
    def __init__(self, filename: str):
        super().__init__(
            filename,
            pagesize=A4,
            leftMargin=MARGIN_X,
            rightMargin=MARGIN_X,
            topMargin=MARGIN_TOP,
            bottomMargin=MARGIN_BOTTOM,
            title="Kensho Excel Lite Retrospective",
            author="Kensho",
            subject="Excel Lite product and autonomous engineering retrospective",
        )
        frame_args = (
            MARGIN_X,
            MARGIN_BOTTOM,
            CONTENT_WIDTH,
            PAGE_HEIGHT - MARGIN_TOP - MARGIN_BOTTOM,
        )
        cover_frame = Frame(*frame_args, id="cover", leftPadding=0, rightPadding=0, topPadding=0, bottomPadding=0)
        body_frame = Frame(*frame_args, id="body", leftPadding=0, rightPadding=0, topPadding=0, bottomPadding=0)
        self.addPageTemplates(
            [
                PageTemplate(id="Cover", frames=[cover_frame], onPage=self._draw_cover),
                PageTemplate(id="Body", frames=[body_frame], onPage=self._draw_body),
            ]
        )
        self._heading_seq = 0

    def beforeDocument(self) -> None:
        self._heading_seq = 0

    def _draw_cover(self, canvas, _doc) -> None:
        canvas.saveState()
        canvas.setFillColor(INK)
        canvas.rect(0, 0, PAGE_WIDTH, PAGE_HEIGHT, stroke=0, fill=1)
        canvas.setFillColor(GREEN)
        canvas.rect(0, PAGE_HEIGHT - 12 * mm, PAGE_WIDTH, 12 * mm, stroke=0, fill=1)
        canvas.setFillColor(colors.HexColor("#213A32"))
        canvas.rect(0, 0, PAGE_WIDTH, 10 * mm, stroke=0, fill=1)
        canvas.restoreState()

    def _draw_body(self, canvas, doc) -> None:
        canvas.saveState()
        canvas.setStrokeColor(GRAY_200)
        canvas.setLineWidth(0.6)
        canvas.line(MARGIN_X, PAGE_HEIGHT - 12 * mm, PAGE_WIDTH - MARGIN_X, PAGE_HEIGHT - 12 * mm)
        canvas.setFont(FONT_BOLD, 7.5)
        canvas.setFillColor(DEEP_GREEN)
        canvas.drawString(MARGIN_X, PAGE_HEIGHT - 9.4 * mm, "KENSHO / EXCEL LITE")
        canvas.setFont(FONT, 7.2)
        canvas.setFillColor(GRAY_500)
        canvas.drawRightString(PAGE_WIDTH - MARGIN_X, PAGE_HEIGHT - 9.4 * mm, "PROJECT RETROSPECTIVE")
        canvas.setStrokeColor(GRAY_200)
        canvas.line(MARGIN_X, 11.5 * mm, PAGE_WIDTH - MARGIN_X, 11.5 * mm)
        canvas.drawString(MARGIN_X, 7.5 * mm, "10 JULY 2026  /  LOCAL-FIRST PRODUCT STUDY")
        canvas.drawRightString(PAGE_WIDTH - MARGIN_X, 7.5 * mm, f"PAGE {doc.page}")
        canvas.restoreState()

    def afterFlowable(self, flowable) -> None:
        if not isinstance(flowable, Paragraph):
            return
        level = {"H2": 0, "H3": 1}.get(flowable.style.name)
        if level is None:
            return
        text = flowable.getPlainText()
        key = f"heading-{self._heading_seq}"
        self._heading_seq += 1
        self.canv.bookmarkPage(key)
        self.canv.addOutlineEntry(text, key, level=level, closed=False)
        if level == 0:
            self.notify("TOCEntry", (level, text, self.page, key))


def inline_markup(value: str) -> str:
    placeholders: dict[str, str] = {}

    def stash(content: str) -> str:
        key = f"@@TOKEN{len(placeholders)}@@"
        placeholders[key] = content
        return key

    def code_repl(match: re.Match[str]) -> str:
        code = html.escape(match.group(1))
        return stash(f'<font name="Courier" color="#145C43">{code}</font>')

    def link_repl(match: re.Match[str]) -> str:
        label = html.escape(match.group(1))
        target = html.escape(match.group(2), quote=True)
        if target.startswith(("https://", "http://")):
            return stash(f'<link href="{target}" color="#145C43"><u>{label}</u></link>')
        return stash(f'<font color="#145C43"><u>{label}</u></font>')

    value = re.sub(r"`([^`]+)`", code_repl, value)
    value = re.sub(r"\[([^]]+)]\(([^)]+)\)", link_repl, value)
    value = html.escape(value)
    value = re.sub(r"\*\*([^*]+)\*\*", rf'<font name="{FONT_BOLD}">\1</font>', value)
    value = re.sub(r"(?<!\*)\*([^*]+)\*(?!\*)", rf'<font name="{FONT_ITALIC}">\1</font>', value)
    for key, content in placeholders.items():
        value = value.replace(key, content)
    return value


def paragraph(text: str, style: str = "body") -> Paragraph:
    return Paragraph(inline_markup(text.strip()), STYLES[style])


def split_table_row(line: str) -> list[str]:
    return [part.strip() for part in line.strip().strip("|").split("|")]


def is_table_separator(line: str) -> bool:
    cells = split_table_row(line)
    return bool(cells) and all(re.fullmatch(r":?-{3,}:?", cell) for cell in cells)


def column_widths(headers: list[str]) -> list[float]:
    normalized = [re.sub(r"[^a-z]+", " ", value.lower()).strip() for value in headers]
    if len(headers) == 2:
        if normalized == ["metric", "value"]:
            ratios = [0.68, 0.32]
        elif normalized[0] in {"gap", "missing check", "signal"}:
            ratios = [0.30, 0.70]
        else:
            ratios = [0.36, 0.64]
    elif len(headers) == 3:
        if "command or probe" in normalized:
            ratios = [0.18, 0.36, 0.46]
        elif normalized[0] == "dimension":
            ratios = [0.22, 0.18, 0.60]
        elif normalized[0] == "role or unit":
            ratios = [0.24, 0.20, 0.56]
        elif normalized[0] in {"finding", "blocker"}:
            ratios = [0.20, 0.36, 0.44]
        elif normalized[0] == "job":
            ratios = [0.18, 0.45, 0.37]
        else:
            ratios = [0.24, 0.30, 0.46]
    else:
        ratios = [1 / len(headers)] * len(headers)
    return [CONTENT_WIDTH * ratio for ratio in ratios]


def make_table(rows: list[list[str]]) -> Table:
    headers = rows[0]
    data: list[list[Paragraph]] = []
    for row_index, row in enumerate(rows):
        padded = (row + [""] * len(headers))[: len(headers)]
        cells = []
        for column_index, value in enumerate(padded):
            style = "table_header" if row_index == 0 else "table_cell"
            if row_index > 0 and headers[column_index].strip().lower() == "value":
                style = "table_cell_right"
            cells.append(paragraph(value, style))
        data.append(cells)
    table = Table(data, colWidths=column_widths(headers), repeatRows=1, splitByRow=1, hAlign="LEFT")
    commands = [
        ("BACKGROUND", (0, 0), (-1, 0), DEEP_GREEN),
        ("BOX", (0, 0), (-1, -1), 0.6, GRAY_300),
        ("INNERGRID", (0, 0), (-1, -1), 0.35, GRAY_200),
        ("VALIGN", (0, 0), (-1, -1), "TOP"),
        ("LEFTPADDING", (0, 0), (-1, -1), 5),
        ("RIGHTPADDING", (0, 0), (-1, -1), 5),
        ("TOPPADDING", (0, 0), (-1, -1), 4.5),
        ("BOTTOMPADDING", (0, 0), (-1, -1), 4.5),
    ]
    for index in range(1, len(data)):
        commands.append(("BACKGROUND", (0, index), (-1, index), WHITE if index % 2 else GRAY_100))
    table.setStyle(TableStyle(commands))
    return table


def make_list(items: list[str], ordered: bool) -> ListFlowable:
    list_items = [
        ListItem(paragraph(item, "list"), leftIndent=10, value=str(index + 1) if ordered else None)
        for index, item in enumerate(items)
    ]
    return ListFlowable(
        list_items,
        bulletType="1" if ordered else "bullet",
        start="1" if ordered else chr(0x2022),
        leftIndent=17,
        bulletFontName=FONT_BOLD,
        bulletFontSize=8,
        bulletColor=GREEN,
        spaceAfter=6,
    )


PAGE_BREAK_SECTIONS = {
    "Evidence Summary",
    "Topology Findings",
    "Framework Bug Ledger",
}


def parse_markdown(source: str) -> list[Flowable]:
    lines = source.splitlines()
    story: list[Flowable] = []
    index = 0
    started = False
    screenshots_inserted = False

    while index < len(lines):
        stripped = lines[index].strip()
        if not started:
            if stripped.startswith("## "):
                started = True
            else:
                index += 1
                continue
        if not stripped:
            index += 1
            continue
        if stripped.startswith("## "):
            heading = stripped[3:].strip()
            if heading == "Evidence Summary" and not screenshots_inserted:
                story.extend(product_evidence())
                screenshots_inserted = True
            if heading in PAGE_BREAK_SECTIONS and story:
                story.append(PageBreak())
            story.append(Paragraph(inline_markup(heading), STYLES["h2"]))
            story.append(Rule(color=MINT, thickness=2.2, space=7))
            index += 1
            continue
        if stripped.startswith("### "):
            story.append(Paragraph(inline_markup(stripped[4:].strip()), STYLES["h3"]))
            index += 1
            continue
        if stripped.startswith("|") and index + 1 < len(lines) and is_table_separator(lines[index + 1].strip()):
            rows = [split_table_row(stripped)]
            index += 2
            while index < len(lines) and lines[index].strip().startswith("|"):
                rows.append(split_table_row(lines[index]))
                index += 1
            story.extend([make_table(rows), Spacer(1, 7)])
            continue
        numbered = re.match(r"^(\d+)\.\s+(.*)$", stripped)
        if numbered:
            items: list[str] = []
            while index < len(lines):
                match = re.match(r"^\s*(\d+)\.\s+(.*)$", lines[index])
                if not match:
                    break
                item = match.group(2).strip()
                index += 1
                continuation: list[str] = []
                while index < len(lines) and lines[index].startswith("   "):
                    continuation.append(lines[index].strip())
                    index += 1
                if continuation:
                    item += " " + " ".join(continuation)
                items.append(item)
                while index < len(lines) and not lines[index].strip():
                    index += 1
            story.append(make_list(items, ordered=True))
            continue
        if stripped.startswith("- "):
            items = []
            while index < len(lines) and lines[index].strip().startswith("- "):
                items.append(lines[index].strip()[2:].strip())
                index += 1
            story.append(make_list(items, ordered=False))
            continue
        paragraph_lines = [stripped]
        index += 1
        while index < len(lines):
            candidate = lines[index].strip()
            if not candidate or candidate.startswith(("#", "- ", "|")) or re.match(r"^\d+\.\s+", candidate):
                break
            paragraph_lines.append(candidate)
            index += 1
        story.append(paragraph(" ".join(paragraph_lines)))
    return story


def cover_story() -> list[Flowable]:
    logo_style = ParagraphStyle("Logo", fontName=FONT_BOLD, fontSize=17, leading=19, textColor=WHITE, alignment=TA_CENTER)
    logo = Table(
        [[Paragraph("XL", logo_style)]],
        colWidths=[14 * mm],
        rowHeights=[14 * mm],
        style=TableStyle([("BACKGROUND", (0, 0), (-1, -1), GREEN), ("VALIGN", (0, 0), (-1, -1), "MIDDLE")]),
        hAlign="LEFT",
    )
    metric_cells = []
    for value, label in [
        ("148", "WORKSHEET FUNCTIONS"),
        ("1,585", "CONFORMANCE CASES"),
        ("188", "KENSHO JOBS"),
        ("1.57", "EFFECTIVE CONCURRENCY"),
    ]:
        metric_cells.append(
            Table(
                [[Paragraph(value, STYLES["cover_metric_value"])], [Paragraph(label, STYLES["cover_metric_label"])]],
                colWidths=[CONTENT_WIDTH / 4 - 5],
                rowHeights=[24, 18],
                style=TableStyle(
                    [
                        ("BACKGROUND", (0, 0), (-1, -1), colors.HexColor("#203D34")),
                        ("VALIGN", (0, 0), (-1, -1), "MIDDLE"),
                        ("LEFTPADDING", (0, 0), (-1, -1), 4),
                        ("RIGHTPADDING", (0, 0), (-1, -1), 4),
                    ]
                ),
            )
        )
    metrics = Table([metric_cells], colWidths=[CONTENT_WIDTH / 4] * 4, hAlign="LEFT")
    metrics.setStyle(TableStyle([("LEFTPADDING", (0, 0), (-1, -1), 2), ("RIGHTPADDING", (0, 0), (-1, -1), 2)]))
    return [
        Spacer(1, 12 * mm),
        logo,
        Spacer(1, 18 * mm),
        Paragraph("KENSHO PROJECT RETROSPECTIVE", STYLES["cover_kicker"]),
        Paragraph("Excel Lite", STYLES["cover_title"]),
        Paragraph(
            "What 188 agent jobs built, where parallelism worked, why integration still converged on Kai, and what must change before a trusted public release.",
            STYLES["cover_deck"],
        ),
        Rule(color=colors.HexColor("#35564B"), space=13),
        Spacer(1, 5 * mm),
        metrics,
        Spacer(1, 18 * mm),
        Paragraph(f'<font name="{FONT_BOLD}" color="#72D2A6">LOCAL DEMO</font>  READY WITH DOCUMENTED CAVEATS', STYLES["cover_meta"]),
        Spacer(1, 3 * mm),
        Paragraph(f'<font name="{FONT_BOLD}" color="#F2B57A">PUBLIC macOS V1</font>  NOT READY', STYLES["cover_meta"]),
        Spacer(1, 14 * mm),
        Paragraph(
            "Analysis date: 10 July 2026<br/>Audited revision: 4f112c930593<br/>Local-first product study / no cloud dependency",
            STYLES["cover_meta"],
        ),
    ]


def status_panel() -> Table:
    definitions = [
        ("LOCAL DEMONSTRATION", "READY", PALE_GREEN, GREEN),
        ("PUBLIC macOS V1", "NOT READY", PALE_AMBER, AMBER),
        ("KENSHO AUTONOMY", "PARTIAL", GRAY_100, GRAY_300),
    ]
    cells = []
    for label, value, background, border in definitions:
        cells.append(
            Table(
                [[Paragraph(label, STYLES["status_label"])], [Paragraph(value, STYLES["status_value"])]],
                style=TableStyle(
                    [
                        ("BACKGROUND", (0, 0), (-1, -1), background),
                        ("BOX", (0, 0), (-1, -1), 0.8, border),
                        ("LEFTPADDING", (0, 0), (-1, -1), 8),
                        ("RIGHTPADDING", (0, 0), (-1, -1), 8),
                        ("TOPPADDING", (0, 0), (-1, -1), 6),
                        ("BOTTOMPADDING", (0, 0), (-1, -1), 6),
                    ]
                ),
            )
        )
    panel = Table([cells], colWidths=[CONTENT_WIDTH / 3] * 3, hAlign="LEFT")
    panel.setStyle(TableStyle([("LEFTPADDING", (0, 0), (-1, -1), 3), ("RIGHTPADDING", (0, 0), (-1, -1), 3)]))
    return panel


def product_evidence() -> list[Flowable]:
    paths = [
        SCREENSHOT_DIR / "after-tauri-recalculation-1120x760.png",
        SCREENSHOT_DIR / "after-packaged-tauri-boot-1120x760.png",
    ]
    if not all(path.exists() for path in paths):
        return []
    image_width = 450
    image_height = image_width * 760 / 1120
    images = [Image(str(path), width=image_width, height=image_height) for path in paths]
    image_tables = []
    for image in images:
        table = Table([[image]], colWidths=[image_width + 4], hAlign="CENTER")
        table.setStyle(
            TableStyle(
                [
                    ("BOX", (0, 0), (-1, -1), 0.6, GRAY_300),
                    ("LEFTPADDING", (0, 0), (-1, -1), 2),
                    ("RIGHTPADDING", (0, 0), (-1, -1), 2),
                    ("TOPPADDING", (0, 0), (-1, -1), 2),
                    ("BOTTOMPADDING", (0, 0), (-1, -1), 2),
                ]
            )
        )
        image_tables.append(table)
    return [
        PageBreak(),
        Paragraph("Product Evidence", STYLES["h2"]),
        Rule(color=MINT, thickness=2.2, space=7),
        paragraph("The native application was exercised directly before and after packaging; these are tracked verification captures, not design mockups."),
        image_tables[0],
        paragraph("Native recalculation workflow", "caption"),
        Spacer(1, 5),
        image_tables[1],
        paragraph("Fresh packaged application boot", "caption"),
    ]


def toc_story() -> list[Flowable]:
    toc = TableOfContents()
    toc.levelStyles = [
        ParagraphStyle("TOC1", fontName=FONT_BOLD, fontSize=9.3, leading=15, textColor=INK, spaceBefore=2),
        ParagraphStyle("TOC2", fontName=FONT, fontSize=8.1, leading=12, leftIndent=13, textColor=GRAY_700),
    ]
    return [
        Paragraph("Contents", STYLES["toc_title"]),
        paragraph(
            "A product verdict and an organizational retrospective, backed by local acceptance evidence and Kensho's recorded execution data."
        ),
        Spacer(1, 5),
        toc,
    ]


def render(source: Path, output: Path) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    body = parse_markdown(source.read_text(encoding="utf-8"))
    for index, flowable in enumerate(body):
        if isinstance(flowable, Paragraph) and flowable.getPlainText() == "Verdict":
            body[index + 2 : index + 2] = [status_panel(), Spacer(1, 8)]
            break
    story: list[Flowable] = []
    story.extend(cover_story())
    story.extend([NextPageTemplate("Body"), PageBreak()])
    story.extend(toc_story())
    story.append(PageBreak())
    story.extend(body)
    RetrospectiveDocTemplate(str(output)).multiBuild(story)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=DEFAULT_SOURCE)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    args = parser.parse_args()
    render(args.source.resolve(), args.output.resolve())
    print(args.output.resolve())


if __name__ == "__main__":
    main()
