#!/usr/bin/env bash
set -euo pipefail

DORY="./target/debug/dory"
PASS=0
FAIL=0

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

assert_contains "version" "dory" "$($DORY --version 2>&1)"

out=$(echo "x" | $DORY --help 2>&1)
assert_contains "help shows run" "run" "$out"
assert_contains "help shows doctor" "doctor" "$out"

out=$($DORY doctor 2>/dev/null)
assert_contains "doctor PHP pass" "[pass] PHP >= 8.4" "$out"
assert_contains "doctor Swoole pass" "[pass] Swoole loaded" "$out"
assert_contains "doctor 45 extensions" "45 loaded" "$out"
assert_contains "doctor ripht" "cli (ripht)" "$out"

# --- Inline eval ---

assert_eq "expr 1+1" "2" "$(echo x | $DORY -r '1 + 1' 2>/dev/null)"
assert_eq "expr strtoupper" '"PHALANX"' "$(echo x | $DORY -r 'strtoupper("phalanx")' 2>/dev/null)"

# --- Bare variables ---

assert_eq "bare var assign+use" "84" "$(echo x | $DORY -r 'x = 42; dump(x * 2)' 2>/dev/null)"
assert_eq "bare var 'is' expands" "42" "$(echo x | $DORY -r 'is = 42; dump(is)' 2>/dev/null)"

out=$(echo x | $DORY -r 'dump(array_map(fn(n) => n * 2, [1,2,3]))' 2>/dev/null)
assert_contains "fn keyword preserved" "4" "$out"

# --- Pipe ---

assert_eq "pipe stdin" "2" "$(echo '1 + 1' | $DORY 2>/dev/null)"

# --- dd ---

code=0; echo x | $DORY -r 'dd("halt")' 2>/dev/null || code=$?
assert_exit "dd exits 0" "0" "$code"

out=$(echo x | $DORY -r 'dd("stop"); dump("never")' 2>/dev/null || true)
assert_contains "dd stops execution" "stop" "$out"

if [[ "$out" == *"never"* ]]; then
    FAIL=$((FAIL + 1))
    echo "FAIL: dd should prevent further execution"
else
    PASS=$((PASS + 1))
fi

# --- dump ---

out=$(echo x | $DORY -r 'dump(["a" => 1, "b" => [2, 3]])' 2>/dev/null)
assert_contains "dump nested array" '"a" => 1' "$out"
assert_contains "dump nested sub" "0 => 2" "$out"

# --- File detection ---

assert_eq "run fixture" "42" "$(echo x | $DORY -r tests/fixtures/return-value.php 2>/dev/null | tr -d '[:space:]')"

code=0; echo x | $DORY -r missing.php 2>&1 || code=$?
assert_exit "missing file exits 1" "1" "$code"

# --- Native function ---

assert_eq "rust ping" '"pong"' "$(echo x | $DORY -r 'dump(dory_rust_ping())' 2>/dev/null)"

# --- HTTP adapter ---

out=$($DORY <<'PHP' 2>/dev/null
$r = dory()->http->get("https://api.github.com/zen");
dump($r->status);
PHP
)
assert_eq "http status" "200" "$out"

# --- FS adapter ---

$DORY <<'PHP' 2>/dev/null
dory()->fs->write("/tmp/dory-gauntlet-assert.txt", "assertion");
PHP

out=$($DORY <<'PHP' 2>/dev/null
dump(dory()->fs->read("/tmp/dory-gauntlet-assert.txt"));
PHP
)
assert_eq "fs read" '"assertion"' "$out"

# --- Concurrent ---

out=$($DORY <<'PHP' 2>/dev/null
$start = microtime(true);
dory()->concurrent(
    function() { \Swoole\Coroutine::sleep(0.1); },
    function() { \Swoole\Coroutine::sleep(0.1); },
    function() { \Swoole\Coroutine::sleep(0.1); },
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

out=$($DORY <<'PHP' 2>/dev/null
dump(dory()->settle(
    fn() => 42,
    fn() => throw new RuntimeException("boom"),
    fn() => "ok",
));
PHP
)
assert_contains "settle summary" "2/3 succeeded" "$out"

# --- Race ---

out=$($DORY <<'PHP' 2>/dev/null
dump(dory()->race(
    function() { \Swoole\Coroutine::sleep(0.2); return "slow"; },
    function() { \Swoole\Coroutine::sleep(0.01); return "fast"; },
));
PHP
)
assert_eq "race fast wins" '"fast"' "$out"

# --- Extensions ---

assert_contains "yaml" "dory" "$(echo x | $DORY -r 'dump(yaml_parse("name: dory"))' 2>/dev/null)"
assert_contains "sodium" "5bb181" "$(echo x | $DORY -r 'dump(bin2hex(sodium_crypto_generichash("phalanx")))' 2>/dev/null)"

out=$($DORY <<'PHP' 2>/dev/null
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
