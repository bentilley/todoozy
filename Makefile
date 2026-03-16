.DEFAULT_GOAL := help

.PHONY: help
help:  ## show this help
	@printf "Usage:\n\tmake [target]\n\nTargets:\n"
	@grep -h "##" $(MAKEFILE_LIST) | sed -E -n 's/^([^:[:space:]]+):[^#]+## (.*)/\t\1:- \2/p' | column -t -s ':'

.PHONY: test
test: ## run tests
	cargo test

.PHONY: bench
bench: ## run parser benchmarks
	cargo bench --features testutils

.PHONY: flamegraph
flamegraph: ## profile todo parsing and generate flamegraph
	CARGO_PROFILE_RELEASE_DEBUG=true cargo flamegraph --example profile_parser --features testutils
