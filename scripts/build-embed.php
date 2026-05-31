<?php

declare(strict_types=1);

/**
 * Generates embedded/dory-runtime.tar from Phalanx monorepo sources.
 *
 * Run: php scripts/build-embed.php
 *
 * Produces a raw tar archive (no PHAR dependency needed at runtime).
 * The Rust binary extracts it to a tmpdir and loads the classmap autoloader.
 */

$toolDir = dirname(__DIR__);
$workspaceRoot = dirname($toolDir, 2);
$monorepoSrc = $workspaceRoot . '/phalanx/src';
$doryRuntime = $workspaceRoot . '/libs/dory-runtime';
$outputPath = $toolDir . '/embedded/dory-runtime.tar';

$monorepoVendor = $workspaceRoot . '/phalanx/vendor';

$packages = [
    ['Phalanx\\', $monorepoSrc . '/Aegis/src'],
    ['Phalanx\\Archon\\', $monorepoSrc . '/Archon/src'],
    ['Phalanx\\Cli\\', $monorepoSrc . '/Cli/src'],
    ['Phalanx\\Themis\\', $monorepoSrc . '/Themis/src'],
    ['Phalanx\\Styx\\', $monorepoSrc . '/Styx/src'],
    ['Phalanx\\Iris\\', $monorepoSrc . '/Iris/src'],
    ['Phalanx\\Grammata\\', $monorepoSrc . '/Grammata/src'],
    ['Phalanx\\Enigma\\', $monorepoSrc . '/Enigma/src'],
    ['Phalanx\\Dory\\', $doryRuntime . '/src'],
    ['Symfony\\Component\\VarDumper\\', $monorepoVendor . '/symfony/var-dumper'],
];

$eagerFiles = [
    'functions.php' => $doryRuntime . '/src/functions.php',
];

if (file_exists($outputPath)) {
    unlink($outputPath);
}

$archive = new PharData($outputPath);
$classMap = [];

foreach ($packages as [$namespace, $srcDir]) {
    if (!is_dir($srcDir)) {
        fwrite(STDERR, "WARN: package source not found: {$srcDir}\n");
        continue;
    }

    $iterator = new RecursiveIteratorIterator(
        new RecursiveDirectoryIterator($srcDir, FilesystemIterator::SKIP_DOTS),
    );

    foreach ($iterator as $file) {
        if ($file->getExtension() !== 'php') {
            continue;
        }

        $realPath = $file->getRealPath();
        $relativePath = substr($realPath, strlen($srcDir) + 1);

        if ($realPath === ($eagerFiles['functions.php'] ?? null)) {
            continue;
        }

        $archivePath = 'src/' . str_replace('\\', '/', $namespace) . $relativePath;
        $archive->addFile($realPath, $archivePath);

        $className = classNameFromPath($namespace, $relativePath);
        if ($className !== null) {
            $classMap[$className] = $archivePath;
        }
    }
}

foreach ($eagerFiles as $name => $path) {
    if (is_file($path)) {
        $archive->addFile($path, $name);
    }
}

$autoloaderContent = generateAutoloader($classMap, array_keys($eagerFiles));
$archive->addFromString('vendor/autoload.php', $autoloaderContent);

$size = filesize($outputPath);
$classCount = count($classMap);
fprintf(STDERR, "Built %s (%s, %d classes)\n", $outputPath, formatBytes($size), $classCount);

function classNameFromPath(string $namespace, string $relativePath): ?string
{
    if (!str_ends_with($relativePath, '.php')) {
        return null;
    }

    $withoutExt = substr($relativePath, 0, -4);
    $parts = explode(DIRECTORY_SEPARATOR, $withoutExt);

    return $namespace . implode('\\', $parts);
}

function generateAutoloader(array $classMap, array $eagerFiles): string
{
    $mapEntries = '';
    ksort($classMap);

    foreach ($classMap as $class => $path) {
        $escapedClass = addslashes($class);
        $escapedPath = addslashes($path);
        $mapEntries .= "    '{$escapedClass}' => __DIR__ . '/../{$escapedPath}',\n";
    }

    $eagerRequires = '';
    foreach ($eagerFiles as $file) {
        $escapedFile = addslashes($file);
        $eagerRequires .= "require __DIR__ . '/../{$escapedFile}';\n";
    }

    return <<<PHP
    <?php

    declare(strict_types=1);

    \$classMap = [
    {$mapEntries}];

    spl_autoload_register(static function (string \$class) use (\$classMap): void {
        if (isset(\$classMap[\$class])) {
            require \$classMap[\$class];
        }
    });

    {$eagerRequires}
    PHP;
}

function formatBytes(int $bytes): string
{
    if ($bytes < 1024) {
        return $bytes . 'B';
    }

    if ($bytes < 1048576) {
        return round($bytes / 1024, 1) . 'KB';
    }

    return round($bytes / 1048576, 1) . 'MB';
}
