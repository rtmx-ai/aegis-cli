# aegis-cli Makefile
# Convenience targets for development workflows.

.PHONY: vhs-tapes

# Record all VHS demo tapes and output GIFs to docs/demos/gifs/
vhs-tapes:
	@./scripts/vhs/run-all.sh
