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

$runtimeDir = getenv('DORY_RUNTIME_DIR');

if ($runtimeDir === false || !is_dir($runtimeDir)) {
    fwrite(STDERR, "Fatal: embedded runtime directory not found.\n");
    exit(126);
}

require $runtimeDir . '/vendor/autoload.php';

// Rust passes args as JSON via DORY_ARGV because the embed SAPI doesn't
// populate $argv or $_SERVER['argv'] as an array the way CLI SAPI does.
$doryArgv = getenv('DORY_ARGV');
$argv = $doryArgv !== false ? json_decode($doryArgv, true) : [];

// Archon expects argv[0] to be the script name (it strips it via array_slice).
array_unshift($argv, 'dory');

$context = [
    'argv' => $argv,
    'env'  => array_filter(
        $_ENV + $_SERVER,
        static fn(string $key): bool => !str_starts_with($key, 'HTTP_'),
        ARRAY_FILTER_USE_KEY,
    ),
];

$exitCode = \Phalanx\Archon\Application\Archon::starting($context)
    ->providers(new \Phalanx\Dory\DoryServiceBundle())
    ->commands(\Phalanx\Dory\Command\DoryCommandGroup::commands())
    ->withConsoleConfig(new \Phalanx\Archon\Application\ConsoleConfig(
        argv: array_slice($argv, 1),
        defaultCommand: 'help',
        scriptName: 'dory',
    ))
    ->run();

// The embed SAPI doesn't propagate exit() codes to the host. Write it to
// a file that the Rust host reads after execution completes.
$exitFile = getenv('DORY_EXIT_FILE');
if ($exitFile !== false) {
    file_put_contents($exitFile, (string) $exitCode);
}

exit($exitCode);
