<?php

declare(strict_types=1);

// For the embed SAPI
if (!defined('STDIN'))  { define('STDIN',  fopen('php://stdin',  'r')); }
if (!defined('STDOUT')) { define('STDOUT', fopen('php://stdout', 'w')); }
if (!defined('STDERR')) { define('STDERR', fopen('php://stderr', 'w')); }

if (!function_exists('phalanx_host_facts')) {
    fwrite(STDERR, "Fatal: host facts native function not registered.\n");
    exit(126);
}

$rawHostFacts = json_decode(phalanx_host_facts(), true);

if (!is_array($rawHostFacts)) {
    fwrite(STDERR, "Fatal: invalid host facts payload.\n");
    exit(126);
}

$runtimeDir = $rawHostFacts['paths']['runtime_dir'] ?? null;

if (!is_string($runtimeDir) || !is_dir($runtimeDir)) {
    fwrite(STDERR, "Fatal: embedded runtime directory not found.\n");
    exit(126);
}

$cwd = $rawHostFacts['paths']['cwd'] ?? null;
if (is_string($cwd) && is_dir($cwd)) {
    chdir($cwd);
}

$functionsFile = $runtimeDir . '/functions.php';
if (is_file($functionsFile) && !function_exists('bia')) {
    require $functionsFile;
}

require $runtimeDir . '/vendor/autoload.php';

try {
    $hostFacts = \Phalanx\Bia\Runtime\Host\HostFacts::hydrate($rawHostFacts);
} catch (\Throwable $e) {
    fwrite(STDERR, $e->getMessage() . "\n");
    exit(126);
}

$_SERVER = [];
$_ENV = [];
$_REQUEST = [];
$_GET = [];
$_POST = [];
$_COOKIE = [];
$_FILES = [];

$exitCode = \Phalanx\Bia\Runtime\BiaCli::fromNativeFacts($hostFacts->toArray())->run();

exit($exitCode);
