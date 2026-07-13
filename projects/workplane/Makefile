SHELL := /bin/sh
PYTHON := .venv/bin/python3
.DEFAULT_GOAL := check

.PHONY: bootstrap generate generated-check check test-contract test-unit test-integration test-browser \
	smoke verify-acceptance verify-release evidence-smoke evidence-acceptance evidence-release clean

bootstrap:
	python3 -m venv .venv
	$(PYTHON) -m pip install --requirement requirements-dev.txt
	npm --prefix web ci
	docker pull golang:1.25.6-alpine@sha256:98e6cffc31ccc44c7c15d83df1d69891efee8115a5bb7ede2bf30a38af3e3c92
	docker pull postgres:17.5-alpine@sha256:6567bca8d7bc8c82c5922425a0baee57be8402df92bae5eacad5f01ae9544daa

generate:
	$(PYTHON) scripts/generate.py
	gofmt -w internal/api/generated/server.gen.go

generated-check:
	$(PYTHON) scripts/generate.py --check

test-contract:
	$(PYTHON) scripts/validate_contracts.py
	$(PYTHON) scripts/contract_selftest.py
	$(PYTHON) scripts/evidence_selftest.py
	$(PYTHON) scripts/gate_selftest.py
	$(PYTHON) scripts/generate.py --check
	$(PYTHON) scripts/generate.py --self-test
	$(PYTHON) scripts/check_hygiene.py

test-unit:
	test -z "$$(gofmt -l cmd internal)"
	go test ./cmd/... ./internal/...
	go vet ./cmd/... ./internal/...
	npm --prefix web run lint
	npm --prefix web run typecheck
	npm --prefix web test
	npm --prefix web run build

test-integration:
	docker compose config --quiet
	scripts/test_postgres.sh
	scripts/test_m1_upgrade.sh
	scripts/test_m1.sh
	scripts/test_realtime.sh
	scripts/test_planning.sh

test-browser:
	scripts/test_m1.sh

check: test-contract test-unit
	docker compose config --quiet
	$(PYTHON) ../../scripts/check_publication.py

smoke:
	$(PYTHON) scripts/gate.py smoke

verify-acceptance:
	$(PYTHON) scripts/gate.py acceptance

verify-release:
	$(PYTHON) scripts/gate.py release

evidence-smoke:
	$(PYTHON) scripts/gate.py smoke --evidence

evidence-acceptance:
	$(PYTHON) scripts/gate.py acceptance --evidence

evidence-release:
	$(PYTHON) scripts/gate.py release --evidence

clean:
	rm -rf web/dist evidence/runs
