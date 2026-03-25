# Plugin provisioning: one command to IL4/IL5 boundary
# Shows: aegis init with GCP plugin, progress events, health checks

Output docs/demos/gifs/05-plugin-provision.gif
Set Width 120
Set Height 35
Set FontSize 14
Set Theme "Catppuccin Mocha"
Set TypingSpeed 50ms
Set Padding 20

Type "aegis init"
Enter
Sleep 1s

# Mode selection: Self-Service BYOC
Type "1"
Enter
Sleep 1s

# Plugin progress events stream:
# KMS KeyRing ... created
# KMS CryptoKey ... created (30-day rotation)
# VPC Network ... created
# VPC-SC Perimeter ... enforced
# Audit Bucket ... created (CMEK)
Sleep 6s

# Health checks:
# kms_key_active ... PASS
# vpc_sc_enforced ... PASS
# audit_sink_flowing ... PASS
# model_accessible ... PASS
Sleep 3s

# Config written, endpoint bound
Sleep 2s

Type "aegis chat"
Enter
Sleep 1s

Type "Hello from an IL5 boundary"
Enter
Sleep 3s
