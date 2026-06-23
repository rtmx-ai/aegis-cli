#!/usr/bin/env bash
# deploy/firewall/aegis-iptables.sh — default-deny egress ruleset (iptables).
#
# iptables equivalent of aegis.nft for hosts without nftables. Defense-in-depth
# BEHIND the app-level zero-egress guarantee (CLAUDE.md §1) — the belt to the
# app's suspenders, not a substitute for it.
#
# Policy: loopback in/out allowed; established/related allowed; ONLY the local
# model port on loopback allowed; all other egress DROPPED by default.
#
# Apply:   sudo MODEL_PORT=8080 bash deploy/firewall/aegis-iptables.sh
# Inspect: sudo iptables -L -n -v ; sudo ip6tables -L -n -v
#
# MODEL_PORT must match calibration.json "port" (default 8080).
set -eu

MODEL_PORT="${MODEL_PORT:-8080}"

apply() {
	ipt="$1"     # iptables or ip6tables
	lo_addr="$2" # 127.0.0.1 or ::1

	# Flush our own chains and set default-deny policies.
	"$ipt" -F
	"$ipt" -P INPUT DROP
	"$ipt" -P FORWARD DROP
	"$ipt" -P OUTPUT DROP

	# Loopback interface: always allowed.
	"$ipt" -A INPUT  -i lo -j ACCEPT
	"$ipt" -A OUTPUT -o lo -j ACCEPT

	# Established/related return traffic.
	"$ipt" -A INPUT  -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT
	"$ipt" -A OUTPUT -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT

	# Only the loopback model port; everything else falls through to DROP policy.
	"$ipt" -A OUTPUT -d "$lo_addr" -p tcp --dport "$MODEL_PORT" -j ACCEPT
	"$ipt" -A INPUT  -d "$lo_addr" -p tcp --dport "$MODEL_PORT" -j ACCEPT
}

if command -v iptables >/dev/null 2>&1; then
	apply iptables 127.0.0.1
	echo "applied iptables default-deny egress (model port $MODEL_PORT on 127.0.0.1)"
fi
if command -v ip6tables >/dev/null 2>&1; then
	apply ip6tables ::1
	echo "applied ip6tables default-deny egress (model port $MODEL_PORT on ::1)"
fi
