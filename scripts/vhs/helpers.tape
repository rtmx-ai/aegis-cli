# Shared VHS helper commands for aegis-cli demos
#
# Source this file from individual tape files to reuse common setup steps.
# Usage: Source "scripts/vhs/helpers.tape"

# Wait for shell prompt to be ready
Sleep 500ms

# Common setup: ensure aegis binary is available
Type "export PATH=$PWD/target/release:$PATH"
Enter
Sleep 500ms

# Verify aegis is available
Type "aegis --version"
Enter
Sleep 1s
