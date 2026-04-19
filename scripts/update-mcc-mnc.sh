#!/usr/bin/env bash
# Refresh lib/data/mcc_mnc.csv from the Wikipedia MCC/MNC list.
# Requires: curl, python3. Only run locally, not in CI.
set -euo pipefail

OUT="$(cd "$(dirname "$0")/.." && pwd)/lib/data/mcc_mnc.csv"
python3 - <<'PY' > "$OUT"
import urllib.request
URL = "https://raw.githubusercontent.com/musalbas/mcc-mnc-table/master/mcc-mnc-table.csv"
raw = urllib.request.urlopen(URL).read().decode("utf-8")
print("mcc,mnc,mnc_is_3_digit,country,operator")
for line in raw.splitlines()[1:]:
    p = line.split(",")
    if len(p) < 6:
        continue
    mcc, mnc = p[0].strip(), p[1].strip()
    if not mcc.isdigit() or not mnc.isdigit():
        continue
    is_3 = "true" if len(mnc) == 3 else "false"
    country = p[4].replace(",", " ").strip()
    op = p[5].replace(",", " ").strip().strip('"')
    print(f"{mcc},{int(mnc)},{is_3},{country},{op}")
PY
echo "wrote $OUT"
