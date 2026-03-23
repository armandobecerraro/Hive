# Uso: make test | make coverage | make check (tests + cobertura 95 % vía Docker)
.PHONY: test coverage check shell

test:
	@chmod +x scripts/docker-check.sh 2>/dev/null; ./scripts/docker-check.sh test

coverage:
	@chmod +x scripts/docker-check.sh 2>/dev/null; ./scripts/docker-check.sh coverage

check:
	@chmod +x scripts/docker-check.sh 2>/dev/null; ./scripts/docker-check.sh all

shell:
	docker compose --profile dev run --rm rust-toolchain-tests bash
