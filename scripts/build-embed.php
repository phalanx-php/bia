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

$classMap = [];
$archiveEntries = [];
$eagerFileRealPaths = eagerFileRealPaths($eagerFiles);

foreach ($packages as [$namespace, $srcDir]) {
    $files = phpFiles($srcDir);

    foreach ($files as $realPath) {
        $relativePath = substr($realPath, strlen($srcDir) + 1);

        if (isset($eagerFileRealPaths[$realPath]) || isPackageTestPath($relativePath)) {
            continue;
        }

        $archivePath = 'src/' . str_replace('\\', '/', $namespace) . $relativePath;
        $archiveEntries[$archivePath] = fileContents($realPath);

        $className = classNameFromPath($namespace, $relativePath);
        if ($className !== null) {
            $classMap[$className] = $archivePath;
        }
    }
}

foreach ($eagerFiles as $name => $path) {
    if (is_file($path)) {
        $archiveEntries[$name] = fileContents($path);
    }
}

$autoloaderContent = generateAutoloader($classMap, array_keys($eagerFiles));
$archiveEntries['vendor/autoload.php'] = $autoloaderContent;

writeTarArchive($outputPath, $archiveEntries);

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

/**
 * @return list<string>
 */
function phpFiles(string $srcDir): array
{
    $files = [];
    $iterator = new RecursiveIteratorIterator(
        new RecursiveDirectoryIterator($srcDir, FilesystemIterator::SKIP_DOTS),
    );

    foreach ($iterator as $file) {
        if ($file->getExtension() !== 'php') {
            continue;
        }

        $realPath = $file->getRealPath();
        if ($realPath !== false) {
            $files[] = $realPath;
        }
    }

    sort($files);

    return $files;
}

function fileContents(string $path): string
{
    $contents = file_get_contents($path);

    if (!is_string($contents)) {
        throw new RuntimeException("Unable to read {$path}.");
    }

    return $contents;
}

/**
 * @param array<string, string> $entries
 */
function writeTarArchive(string $outputPath, array $entries): void
{
    ksort($entries);

    $handle = fopen($outputPath, 'wb');

    if (!is_resource($handle)) {
        throw new RuntimeException("Unable to open {$outputPath} for writing.");
    }

    foreach ($entries as $path => $contents) {
        writeTarEntry($handle, $path, $contents);
    }

    fwrite($handle, str_repeat("\0", 1024));
    fclose($handle);
}

/**
 * @param resource $handle
 */
function writeTarEntry($handle, string $path, string $contents): void
{
    [$name, $prefix] = tarNameParts($path);

    $header = str_repeat("\0", 512);
    writeTarField($header, 0, 100, $name);
    writeTarField($header, 100, 8, tarOctal(0644, 7));
    writeTarField($header, 108, 8, tarOctal(0, 7));
    writeTarField($header, 116, 8, tarOctal(0, 7));
    writeTarField($header, 124, 12, tarOctal(strlen($contents), 11));
    writeTarField($header, 136, 12, tarOctal(0, 11));
    writeTarField($header, 148, 8, '        ');
    writeTarField($header, 156, 1, '0');
    writeTarField($header, 257, 6, "ustar\0");
    writeTarField($header, 263, 2, '00');
    writeTarField($header, 345, 155, $prefix);

    writeTarField($header, 148, 8, str_pad(decoct(tarChecksum($header)), 6, '0', STR_PAD_LEFT) . "\0 ");

    fwrite($handle, $header);
    fwrite($handle, $contents);

    $padding = strlen($contents) % 512;
    if ($padding !== 0) {
        fwrite($handle, str_repeat("\0", 512 - $padding));
    }
}

/**
 * @return array{string, string}
 */
function tarNameParts(string $path): array
{
    $path = str_replace('\\\\', '/', $path);

    if (strlen($path) <= 100) {
        return [$path, ''];
    }

    $offset = strlen($path);
    while (($offset = strrpos(substr($path, 0, $offset), '/')) !== false) {
        $prefix = substr($path, 0, $offset);
        $name = substr($path, $offset + 1);

        if (strlen($prefix) <= 155 && strlen($name) <= 100) {
            return [$name, $prefix];
        }
    }

    throw new RuntimeException("Tar path is too long: {$path}.");
}

function tarOctal(int $value, int $digits): string
{
    return str_pad(decoct($value), $digits, '0', STR_PAD_LEFT) . "\0";
}

function tarChecksum(string $header): int
{
    $checksum = 0;

    for ($i = 0; $i < 512; $i++) {
        $checksum += ord($header[$i]);
    }

    return $checksum;
}

function writeTarField(string &$header, int $offset, int $length, string $value): void
{
    $header = substr_replace(
        $header,
        substr(str_pad($value, $length, "\0"), 0, $length),
        $offset,
        $length,
    );
}
