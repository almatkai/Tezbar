#!/bin/bash

START=$(date +%s.%N)
FIRST_TOKEN_TIME=""
TOKEN_COUNT=0

curl -N -s https://llm.alem.ai/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer ${ALEM_API_KEY}" \
  -d @request.json | while IFS= read -r line; do
    if [[ "$line" == data:* ]] && [[ "$line" != "data: [DONE]" ]]; then
      NOW=$(date +%s.%N)
      if [ -z "$FIRST_TOKEN_TIME" ]; then
        FIRST_TOKEN_TIME=$NOW
        TTFT=$(echo "$FIRST_TOKEN_TIME - $START" | bc)
        echo -e "\n\n[TTFT: ${TTFT}s]" >&2
      fi
      CONTENT=$(echo "$line" | sed 's/^data: //' | python3 -c "import sys,json; 
try:
  d=json.load(sys.stdin)
  c=d['choices'][0]['delta'].get('content','')
  print(c, end='')
except: pass" 2>/dev/null)
      printf '%s' "$CONTENT"
      TOKEN_COUNT=$((TOKEN_COUNT+1))
    fi
  done

END=$(date +%s.%N)
TOTAL=$(echo "$END - $START" | bc)
echo -e "\n\n[Total time: ${TOTAL}s]" >&2
