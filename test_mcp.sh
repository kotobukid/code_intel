#!/bin/bash

echo "=== Testing MCP Client ==="

# Test 1: Initialize
echo -e "\n1. Testing initialize:"
echo '{"jsonrpc":"2.0","method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{"tools":{}}},"id":1}' | \
    /home/kakehashi/RustroverProjects/code_intel/target/release/code_intel mcp-client 2>&1

# Test 2: List tools
echo -e "\n2. Testing tools/list:"
echo '{"jsonrpc":"2.0","method":"tools/list","params":{},"id":2}' | \
    /home/kakehashi/RustroverProjects/code_intel/target/release/code_intel mcp-client 2>&1

# Test 3: Call find_definition
echo -e "\n3. Testing tools/call (find_definition for 'add'):"
echo '{"jsonrpc":"2.0","method":"tools/call","params":{"name":"find_definition","arguments":{"function_name":"add"}},"id":3}' | \
    /home/kakehashi/RustroverProjects/code_intel/target/release/code_intel mcp-client 2>&1

echo -e "\n=== Test Complete ==="