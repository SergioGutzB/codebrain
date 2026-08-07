# CodeBrain agent task runner — requires https://github.com/casey/just
# Fallback: call scripts under scripts/agent/ directly.

set shell := ["bash", "-cu"]

default:
	@just --list

pre:
	./scripts/agent/preflight.sh

validate:
	./scripts/agent/validate.sh

post:
	./scripts/agent/postcheck.sh

hooks:
	./scripts/agent/install-hooks.sh

commit-msg msg:
	echo "{{msg}}" | ./scripts/agent/check-commit-msg.sh

fmt:
	cargo fmt --all

check:
	cargo check --workspace

build:
	cargo build -p codebrain-cli --release

# Fixture micro-bench (CI smoke)
bench:
	./scripts/bench-index.sh

# GA synthetic target (~1k files + ~2k notes). Override with e.g. `just bench-ga files=500 notes=500`
bench-ga files="1000" notes="2000" out="/tmp/codebrain-bench-ga":
	./scripts/bench-index.sh --synthetic --files {{files}} --notes {{notes}} --out {{out}}

doctor config="codebrain.toml":
	cargo run -q -p codebrain-cli -- --config {{config}} doctor

index config="codebrain.toml":
	cargo run -q -p codebrain-cli -- --config {{config}} index
