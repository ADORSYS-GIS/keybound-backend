#!/bin/bash
# Swagger UI Helper Script
# This script helps you test the id_document flow using curl commands
# You'll need to enter the token in Swagger UI manually

echo "======================================"
echo "Swagger UI Helper for id_document Flow"
echo "======================================"
echo ""
echo "Steps to test from Swagger UI:"
echo ""
echo "1. Get your Bearer Token:"
echo "   -> Open: http://localhost:9026/realms/e2e-testing/protocol/openid-connect/token"
echo "   -> Method: POST"
echo "   -> Body (form-urlencoded):"
echo "      grant_type=client_credentials"
echo "      client_id=test-client"
echo "      client_secret=some-secret"
echo "   -> Copy the 'access_token' from the response"
echo ""
echo "2. In Swagger UI (http://localhost:3001/swagger-ui/):"
echo "   -> Click 'Authorize' button"
echo "   -> Enter: Bearer <your_token>"
echo "   -> Click 'Authorize' and then 'Close'"
echo ""
echo "3. Testing the id_document flow:"
echo "   a) Create Session:"
echo "      POST /bff/sessions"
echo "      Body: {\"sessionType\": \"kyc_full\"}"
echo ""
echo "   b) Add id_document flow:"
echo "      POST /bff/sessions/{sessionId}/flows"
echo "      Body: {\"flowType\": \"id_document\"}"
echo ""
echo "   c) Get the step ID from response and submit:"
echo "      POST /bff/steps/{stepId}"
echo "      Body: {\"action\": \"submit\"}"
echo ""
echo "   d) Staff approval (from Staff API):"
echo "      POST /staff/flow/steps/{stepId}"
echo "      Body: {\"action\": \"approve\"}"
echo ""
echo "======================================"
echo "Quick Token Command:"
echo "======================================"

# Print a ready-to-use curl command
echo ""
echo "Run this command to get a token:"
echo ""
echo 'TOKEN=$(curl -s -X POST "http://localhost:9026/realms/e2e-testing/protocol/openid-connect/token" \'
echo '  -H "Content-Type: application/x-www-form-urlencoded" \'
echo '  -d "grant_type=client_credentials" \'
echo '  -d "client_id=test-client" \'
echo '  -d "client_secret=some-secret" | python3 -c "import sys,json; print(json.load(sys.stdin).get(\"access_token\",\"\"))")'
echo ""
echo "Then use the token in Swagger Authorize button."