#!/usr/bin/env python3
import json
import sys

def main():
    # Read from stdin
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
            
        try:
            request = json.loads(line)
            
            if request.get("method") == "initialize":
                response = {
                    "jsonrpc": "2.0",
                    "result": {
                        "protocolVersion": "2024-11-05",
                        "capabilities": {
                            "tools": {},
                            "resources": {},
                            "prompts": {}
                        },
                        "serverInfo": {
                            "name": "test-server",
                            "version": "1.0.0"
                        }
                    },
                    "id": request.get("id")
                }
                print(json.dumps(response), flush=True)
            elif request.get("method") == "initialized":
                # No response needed
                pass
            elif request.get("method") == "ping":
                response = {
                    "jsonrpc": "2.0",
                    "result": {},
                    "id": request.get("id")
                }
                print(json.dumps(response), flush=True)
                
        except Exception as e:
            # Silent error
            pass

if __name__ == "__main__":
    main()