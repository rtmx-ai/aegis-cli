#!/usr/bin/env bash
# Minimal mock MCP server for integration tests.
# Speaks JSON-RPC 2.0 over stdin/stdout (NDJSON).
# Responds to: initialize, tools/list, tools/call

while IFS= read -r line; do
    method=$(echo "$line" | python3 -c "import sys,json; print(json.load(sys.stdin).get('method',''))" 2>/dev/null)
    id=$(echo "$line" | python3 -c "import sys,json; print(json.load(sys.stdin).get('id','null'))" 2>/dev/null)

    case "$method" in
        "initialize")
            echo "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"protocolVersion\":\"2024-11-05\",\"serverInfo\":{\"name\":\"mock-server\",\"version\":\"1.0\"}}}"
            ;;
        "notifications/initialized")
            # Notification -- no response
            ;;
        "tools/list")
            echo "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"tools\":[{\"name\":\"echo\",\"description\":\"Echo the input back\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"message\":{\"type\":\"string\"}}}}]}}"
            ;;
        "tools/call")
            msg=$(echo "$line" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('params',{}).get('arguments',{}).get('message','hello'))" 2>/dev/null)
            echo "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"content\":[{\"type\":\"text\",\"text\":\"echo: $msg\"}]}}"
            ;;
        *)
            # Unknown method -- ignore
            ;;
    esac
done
