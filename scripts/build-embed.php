<?php

declare(strict_types=1);

/**
 * Generates embedded/bia-runtime.tar from Phalanx monorepo sources.
 *
 * Run: php scripts/build-embed.php
 *
 * Produces a raw tar archive (no PHAR dependency needed at runtime).
 * The Rust binary extracts it to a tmpdir and loads the classmap autoloader.
 */

$toolDir = dirname(__DIR__);
$workspaceRoot = dirname($toolDir, 2);
$outputPath = $toolDir . '/embedded/bia-runtime.tar';

$matrix = require $toolDir . '/embedded/matrix.php';
$packages = matrixPackages($matrix, $workspaceRoot);
$eagerFiles = matrixEagerFiles($matrix, $workspaceRoot);

$missingPackageDirs = [];

foreach ($packages as [$namespace, $srcDir]) {
    if (!is_dir($srcDir)) {
        $missingPackageDirs[] = "{$namespace} => {$srcDir}";
    }
}

if ($missingPackageDirs !== []) {
    fwrite(STDERR, "ERROR: required package source directories are missing:\n");

    foreach ($missingPackageDirs as $missingPackageDir) {
        fwrite(STDERR, "  - {$missingPackageDir}\n");
    }

    exit(1);
}

assertCurrentModulesAreAccounted($matrix, $workspaceRoot);

$missingEagerFiles = [];

foreach ($eagerFiles as $archivePath => $path) {
    if (!is_file($path)) {
        $missingEagerFiles[] = "{$archivePath} => {$path}";
    }
}

if ($missingEagerFiles !== []) {
    fwrite(STDERR, "ERROR: required eager files are missing:\n");

    foreach ($missingEagerFiles as $missingEagerFile) {
        fwrite(STDERR, "  - {$missingEagerFile}\n");
    }

    exit(1);
}

if (file_exists($outputPath)) {
    unlink($outputPath);
}

$archive = new PharData($outputPath);
$classMap = [];
$eagerFileRealPaths = eagerFileRealPaths($eagerFiles);

foreach ($packages as [$namespace, $srcDir]) {
    $iterator = new RecursiveIteratorIterator(
        new RecursiveDirectoryIterator($srcDir, FilesystemIterator::SKIP_DOTS),
    );

    foreach ($iterator as $file) {
        if ($file->getExtension() !== 'php') {
            continue;
        }

        $realPath = $file->getRealPath();
        $relativePath = substr($realPath, strlen($srcDir) + 1);

        if (isset($eagerFileRealPaths[$realPath]) || isPackageTestPath($relativePath)) {
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

function isPackageTestPath(string $relativePath): bool
{
    return str_starts_with($relativePath, 'tests' . DIRECTORY_SEPARATOR);
}

/**
 * @param array<string, mixed> $matrix
 * @return list<array{string, string}>
 */
function matrixPackages(array $matrix, string $workspaceRoot): array
{
    $packages = [];

    foreach ($matrix['packages'] ?? [] as $entry) {
        if (!is_array($entry) || !isset($entry['namespace'], $entry['path'])) {
            throw new RuntimeException('Invalid Bia embed matrix package entry.');
        }

        $packages[] = [$entry['namespace'], resolveMatrixPath($workspaceRoot, $entry['path'])];
    }

    return $packages;
}

/**
 * @param array<string, mixed> $matrix
 * @return array<string, string>
 */
function matrixEagerFiles(array $matrix, string $workspaceRoot): array
{
    $files = [];

    foreach ($matrix['eager_files'] ?? [] as $archivePath => $path) {
        if (!is_string($archivePath) || !is_string($path)) {
            throw new RuntimeException('Invalid Bia embed matrix eager file entry.');
        }

        $files[$archivePath] = resolveMatrixPath($workspaceRoot, $path);
    }

    return $files;
}

function resolveMatrixPath(string $workspaceRoot, string $path): string
{
    return $workspaceRoot . '/' . ltrim($path, '/');
}

/** @param array<string, mixed> $matrix */
function assertCurrentModulesAreAccounted(array $matrix, string $workspaceRoot): void
{
    $modules = $matrix['modules'] ?? null;

    if (!is_array($modules)) {
        throw new RuntimeException('Bia embed matrix must account for current Phalanx modules.');
    }

    $missing = [];

    foreach (glob($workspaceRoot . '/phalanx/src/*/composer.json') ?: [] as $composerPath) {
        $module = basename(dirname($composerPath));

        if (!array_key_exists($module, $modules)) {
            $missing[] = $module;
        }
    }

    if ($missing === []) {
        return;
    }

    sort($missing);
    fwrite(STDERR, "ERROR: Bia embed matrix does not account for current modules:\n");

    foreach ($missing as $module) {
        fwrite(STDERR, "  - {$module}\n");
    }

    exit(1);
}

/**
 * @param array<string, string> $eagerFiles
 * @return array<string, true>
 */
function eagerFileRealPaths(array $eagerFiles): array
{
    $realPaths = [];

    foreach ($eagerFiles as $path) {
        $realPath = realpath($path);

        if ($realPath !== false) {
            $realPaths[$realPath] = true;
        }
    }

    return $realPaths;
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
