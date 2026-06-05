#!/usr/bin/env bash
set -euo pipefail

BIA="${BIA_BIN:-./target/debug/bia}"
PASS=0
FAIL=0
WORKDIR="$(mktemp -d)"
HTTP_PID=""

cleanup() {
    if [[ -n "$HTTP_PID" ]]; then
        kill "$HTTP_PID" 2>/dev/null || true
        wait "$HTTP_PID" 2>/dev/null || true
    fi

    rm -rf "$WORKDIR"
}

trap cleanup EXIT

assert_eq() {
    local label="$1" expected="$2" actual="$3"

    if [[ "$actual" == "$expected" ]]; then
        PASS=$((PASS + 1))
    else
        FAIL=$((FAIL + 1))
        echo "FAIL: $label"
        echo "  expected: $expected"
        echo "  actual:   $actual"
    fi
}

assert_contains() {
    local label="$1" needle="$2" haystack="$3"

    if [[ "$haystack" == *"$needle"* ]]; then
        PASS=$((PASS + 1))
    else
        FAIL=$((FAIL + 1))
        echo "FAIL: $label"
        echo "  expected to contain: $needle"
        echo "  actual: $haystack"
    fi
}

assert_exit() {
    local label="$1" expected="$2" actual="$3"

    if [[ "$actual" == "$expected" ]]; then
        PASS=$((PASS + 1))
    else
        FAIL=$((FAIL + 1))
        echo "FAIL: $label (exit code)"
        echo "  expected: $expected"
        echo "  actual:   $actual"
    fi
}

# --- CLI ---

assert_contains "version" "bia" "$($BIA --version 2>&1)"

out=$(echo "x" | $BIA --help 2>&1)
assert_contains "help shows run" "run" "$out"
assert_contains "help shows doctor" "doctor" "$out"

out=$($BIA doctor 2>/dev/null)
assert_contains "doctor PHP pass" "[pass] PHP >= 8.4" "$out"
assert_contains "doctor Swoole pass" "[pass] Swoole loaded" "$out"
assert_contains "doctor extensions loaded" "loaded" "$out"
assert_contains "doctor ripht" "cli (ripht)" "$out"

# --- Inline eval ---

assert_eq "expr 1+1" "2" "$(echo x | $BIA -r '1 + 1' 2>/dev/null)"
assert_eq "expr strtoupper" '"PHALANX"' "$(echo x | $BIA -r 'strtoupper("phalanx")' 2>/dev/null)"

# --- Bare variables ---

assert_eq "bare var assign+use" "84" "$(echo x | $BIA -r 'x = 42; dump(x * 2)' 2>/dev/null)"
assert_eq "bare var 'is' expands" "42" "$(echo x | $BIA -r 'is = 42; dump(is)' 2>/dev/null)"

out=$(echo x | $BIA -r 'dump(array_map(fn(n) => n * 2, [1,2,3]))' 2>/dev/null)
assert_contains "fn keyword preserved" "4" "$out"

# --- Pipe ---

assert_eq "pipe stdin" "2" "$(echo '1 + 1' | $BIA 2>/dev/null)"

# --- dd ---

code=0; echo x | $BIA -r 'dd("halt")' 2>/dev/null || code=$?
assert_exit "dd exits 0" "0" "$code"

out=$(echo x | $BIA -r 'dd("stop"); dump("never")' 2>/dev/null || true)
assert_contains "dd stops execution" "stop" "$out"

if [[ "$out" == *"never"* ]]; then
    FAIL=$((FAIL + 1))
    echo "FAIL: dd should prevent further execution"
else
    PASS=$((PASS + 1))
fi

# --- dump ---

out=$(echo x | $BIA -r 'dump(["a" => 1, "b" => [2, 3]])' 2>/dev/null)
assert_contains "dump nested array" '"a" => 1' "$out"
assert_contains "dump nested sub" "0 => 2" "$out"

# --- File detection ---

assert_eq "run fixture" "42" "$(echo x | $BIA -r tests/fixtures/return-value.php 2>/dev/null | tr -d '[:space:]')"

code=0; echo x | $BIA -r missing.php 2>&1 || code=$?
assert_exit "missing file exits 1" "1" "$code"

# --- Code parser ---

assert_eq "code query native" "true" "$(echo x | $BIA -r 'dump(function_exists("bia_code_query_json"))' 2>/dev/null)"
assert_eq "code parser declaration" '"Demo"' "$(echo x | $BIA -r 'dump(bia()->code->parse("final class Demo {}", "demo.php")->declarations[0]->name)' 2>/dev/null)"

mkdir -p "$WORKDIR/code/src" "$WORKDIR/code/vendor"
printf '%s' '<?php namespace App; final class Example { public function run(): void {} }' > "$WORKDIR/code/src/Example.php"
printf '%s' '<?php class Ignored {}' > "$WORKDIR/code/vendor/Ignored.php"

out=$(BIA_GAUNTLET_CODE_ROOT="$WORKDIR/code" $BIA <<'PHP' 2>/dev/null
$root = getenv("BIA_GAUNTLET_CODE_ROOT");
dump(bia()->code->indexProject($root)->fileCount);
dump(bia()->code->declarations($root, new \Phalanx\Bia\Code\DeclarationQuery(kind: "class"))->declarations[0]->file);
dump(bia()->code->tokens($root, new \Phalanx\Bia\Code\TokenQuery(text: "class"))->tokens[0]->text);
PHP
)
assert_contains "code project index excludes vendor" "1" "$out"
assert_contains "code declaration query file" '"src/Example.php"' "$out"
assert_contains "code token query text" '"class"' "$out"

out=$($BIA code check "$WORKDIR/code" 2>/dev/null)
assert_contains "code check command files" "Files: 1" "$out"
assert_contains "code check command pass" "[pass] no parse errors" "$out"

out=$($BIA code declarations "$WORKDIR/code" --kind=class --name=Example 2>/dev/null)
assert_contains "code declarations command" "class App\\Example src/Example.php:1" "$out"

out=$($BIA code tokens "$WORKDIR/code" --text=class --file=src/Example.php --json 2>/dev/null)
assert_contains "code tokens command json kind" '"kind": "Class"' "$out"
assert_contains "code tokens command json text" '"text": "class"' "$out"

code=0; $BIA code check "$WORKDIR/missing-code-root" >/dev/null 2>&1 || code=$?
assert_exit "code check missing root exits 1" "1" "$code"

code=0; $BIA code tokens "$WORKDIR/code" --json >/dev/null 2>&1 || code=$?
assert_exit "code tokens unfiltered exits 1" "1" "$code"

mkdir -p "$WORKDIR/bad-code"
printf '%s' '<?php class {' > "$WORKDIR/bad-code/Broken.php"
code=0; out=$($BIA code check "$WORKDIR/bad-code" 2>/dev/null) || code=$?
assert_exit "code check parse errors exits 1" "1" "$code"
assert_contains "code check parse errors reports fail" "[fail] parse errors" "$out"

# --- HTTP adapter ---

printf 'bia-local\n' > "$WORKDIR/zen"
python3 - "$WORKDIR" "$WORKDIR/http-port" > "$WORKDIR/http.log" 2>&1 <<'PY' &
import functools
import http.server
import pathlib
import socketserver
import sys

root = sys.argv[1]
port_file = pathlib.Path(sys.argv[2])
handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=root)

with socketserver.TCPServer(("127.0.0.1", 0), handler) as httpd:
    port_file.write_text(str(httpd.server_address[1]), encoding="utf-8")
    httpd.serve_forever()
PY
HTTP_PID=$!

for _ in {1..50}; do
    if [[ -s "$WORKDIR/http-port" ]]; then
        break
    fi

    sleep 0.1
done

if [[ ! -s "$WORKDIR/http-port" ]]; then
    FAIL=$((FAIL + 1))
    echo "FAIL: local HTTP fixture did not start"
else
    HTTP_PORT="$(cat "$WORKDIR/http-port")"

out=$(BIA_GAUNTLET_HTTP_URL="http://127.0.0.1:${HTTP_PORT}/zen" $BIA <<'PHP' 2>/dev/null
$r = bia()->http->get(getenv("BIA_GAUNTLET_HTTP_URL"));
dump($r->status);
PHP
)
assert_eq "http status" "200" "$out"
fi

# --- FS adapter ---

BIA_GAUNTLET_FS_PATH="$WORKDIR/bia-gauntlet-assert.txt" $BIA <<'PHP' 2>/dev/null
bia()->fs->write(getenv("BIA_GAUNTLET_FS_PATH"), "assertion");
PHP

out=$(BIA_GAUNTLET_FS_PATH="$WORKDIR/bia-gauntlet-assert.txt" $BIA <<'PHP' 2>/dev/null
dump(bia()->fs->read(getenv("BIA_GAUNTLET_FS_PATH")));
PHP
)
assert_eq "fs read" '"assertion"' "$out"

# --- Concurrent ---

out=$($BIA <<'PHP' 2>/dev/null
$start = microtime(true);
bia()->concurrent(
    fn($s) => $s->delay(0.1),
    fn($s) => $s->delay(0.1),
    fn($s) => $s->delay(0.1),
);
$ms = round((microtime(true) - $start) * 1000);
echo $ms;
PHP
)

if [[ "$out" -lt 200 ]]; then
    PASS=$((PASS + 1))
else
    FAIL=$((FAIL + 1))
    echo "FAIL: concurrent should complete in <200ms, got ${out}ms"
fi

# --- Settle ---

out=$($BIA <<'PHP' 2>/dev/null
dump(bia()->settle(
    fn($_s) => 42,
    fn($_s) => throw new RuntimeException("boom"),
    fn($_s) => "ok",
));
PHP
)
assert_contains "settle summary" "2/3 succeeded" "$out"

# --- Race ---

out=$($BIA <<'PHP' 2>/dev/null
dump(bia()->race(
    fn($p) => (function() use ($p) { $p->delay(0.2); return "slow"; })(),
    fn($p) => (function() use ($p) { $p->delay(0.01); return "fast"; })(),
));
PHP
)
assert_eq "race fast wins" '"fast"' "$out"

# --- Extensions ---

assert_contains "yaml" "bia" "$(echo x | $BIA -r 'dump(yaml_parse("name: bia"))' 2>/dev/null)"
assert_contains "sodium" "5bb181" "$(echo x | $BIA -r 'dump(bin2hex(sodium_crypto_generichash("phalanx")))' 2>/dev/null)"

out=$($BIA <<'PHP' 2>/dev/null
$db = new SQLite3(":memory:");
$db->exec("CREATE TABLE t (v TEXT)");
$db->exec("INSERT INTO t VALUES ('ok')");
dump($db->querySingle("SELECT v FROM t"));
PHP
)
assert_eq "sqlite" '"ok"' "$out"

# --- Summary ---

echo ""
echo "=== Gauntlet: $PASS passed, $FAIL failed ==="

if [[ $FAIL -gt 0 ]]; then
    exit 1
fi
