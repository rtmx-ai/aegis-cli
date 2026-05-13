# rtmx:req REQ-BUILD-021
# Plugin provisioning demo: aegis-infra/v1 plugin lifecycle.
#
# Demonstrates the full plugin lifecycle:
#   1. Plugin discovery via manifest subcommand.
#   2. Preview the infrastructure changes.
#   3. Provision with progress events streaming.
#   4. Status check shows health of provisioned resources.
#
# Output: docs/demos/plugin-provision.gif
# Regenerate: vhs docs/demos/tapes/plugin-provision.tape

Output docs/demos/plugin-provision.gif

Set Width 1200
Set Height 700
Set FontSize 14
Set Theme "Catppuccin Mocha"
Set TypingSpeed 50ms
Set Padding 20

# 1. Initialize aegis with a cloud provider
Type "aegis init"
Enter
Sleep 1s

# Mode selection: Self-Service BYOC
Type "1"
Enter
Sleep 1s

# 2. Plugin progress events stream via NDJSON:
# KMS KeyRing ... created
# KMS CryptoKey ... created (30-day rotation)
# VPC Network ... created
# VPC-SC Perimeter ... enforced
# Audit Bucket ... created (CMEK)
Sleep 6s

# 3. Health checks:
# kms_key_active ... PASS
# vpc_sc_enforced ... PASS
# audit_sink_flowing ... PASS
# model_accessible ... PASS
Sleep 3s

# Config written, endpoint bound
Sleep 2s

# 4. Launch chat against provisioned endpoint
Type "aegis chat"
Enter
Sleep 1s

Type "Hello from an IL5 boundary"
Enter
Sleep 3s
