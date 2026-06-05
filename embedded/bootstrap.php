<?php

declare(strict_types=1);

// For the embed SAPI
if (!defined('STDIN'))  { define('STDIN',  fopen('php://stdin',  'r')); }
if (!defined('STDOUT')) { define('STDOUT', fopen('php://stdout', 'w')); }
if (!defined('STDERR')) { define('STDERR', fopen('php://stderr', 'w')); }

// Restore the user's working directory — the embed SAPI may not inherit it.
$pwd = $_SERVER['PWD'] ?? getenv('PWD');
if ($pwd !== false && is_dir($pwd)) {
    chdir($pwd);
}

$runtimeDir = getenv('BIA_RUNTIME_DIR');

if ($runtimeDir === false || !is_dir($runtimeDir)) {
    fwrite(STDERR, "Fatal: embedded runtime directory not found.\n");
    exit(126);
}

require $runtimeDir . '/vendor/autoload.php';

// Rust passes args as JSON via BIA_ARGV because the embed SAPI doesn't
// populate $argv or $_SERVER['argv'] as an array the way CLI SAPI does.
$biaArgv = getenv('BIA_ARGV');
$argv = $biaArgv !== false ? json_decode($biaArgv, true) : [];

// Console expects argv[0] to be the script name (it strips it via array_slice).
array_unshift($argv, 'bia');

$env = array_filter(
    $_ENV + $_SERVER,
    static fn(string $key): bool => !str_starts_with($key, 'HTTP_'),
    ARRAY_FILTER_USE_KEY,
);

foreach (['BIA_SCRIPT_TIMEOUT', 'BIA_MAX_CONCURRENCY', 'BIA_VERBOSE', 'BIA_EMBEDDED'] as $key) {
    $value = getenv($key);

    if ($value !== false) {
        $env[$key] = $value;
    }
}

$projectConfig = \Phalanx\Bia\Runtime\BiaProjectConfig::discover(getcwd() ?: '.');

$context = [
    ...$projectConfig->contextOverlay(),
    ...$env,
    'argv' => $argv,
];

$exitCode = \Phalanx\Console\Application\Console::starting($context)
    ->providers(
        new \Phalanx\Bia\Runtime\BiaServiceBundle(),
        new \Phalanx\HttpClient\HttpServiceBundle(),
        new \Phalanx\Filesystem\FilesystemServiceBundle(),
        new \Phalanx\Network\NetworkServiceBundle(),
        new \Phalanx\WebSocket\WsServiceBundle(),
    )
    ->commands(\Phalanx\Bia\Command\BiaCommandGroup::commands())
    ->withErrorRenderers(new \Phalanx\Bia\Console\ScriptFaultRenderer())
    ->withConsoleConfig(new \Phalanx\Console\Application\ConsoleConfig(
        argv: array_slice($argv, 1),
        defaultCommand: 'help',
        scriptName: 'bia',
    ))
    ->run();

// The embed SAPI doesn't propagate exit() codes to the host. Write it to
// a file that the Rust host reads after execution completes.
$exitFile = getenv('BIA_EXIT_FILE');
if ($exitFile !== false) {
    file_put_contents($exitFile, (string) $exitCode);
}

exit($exitCode);
