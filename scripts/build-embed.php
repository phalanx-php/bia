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
    ['Phalanx\\Stoa\\', $monorepoSrc . '/Stoa/src'],
    ['Phalanx\\Hydra\\', $monorepoSrc . '/Hydra/src'],
    ['Phalanx\\Skopos\\', $monorepoSrc . '/Skopos/src'],
    ['Phalanx\\Hermes\\', $monorepoSrc . '/Hermes/src'],
    ['Phalanx\\Argos\\', $monorepoSrc . '/Argos/src'],
    ['Phalanx\\Dory\\', $doryRuntime . '/src'],
    ['GuzzleHttp\\Psr7\\', $monorepoVendor . '/guzzlehttp/psr7/src'],
    ['Psr\\Http\\Client\\', $monorepoVendor . '/psr/http-client/src'],
    ['Psr\\Http\\Message\\', $monorepoVendor . '/psr/http-factory/src'],
    ['Psr\\Http\\Message\\', $monorepoVendor . '/psr/http-message/src'],
    ['Psr\\Container\\', $monorepoVendor . '/psr/container/src'],
    ['Psr\\EventDispatcher\\', $monorepoVendor . '/psr/event-dispatcher/src'],
    ['Psr\\Log\\', $monorepoVendor . '/psr/log/src'],
    ['Psr\\SimpleCache\\', $monorepoVendor . '/psr/simple-cache/src'],
    ['FastRoute\\', $monorepoVendor . '/nikic/fast-route/src'],
    ['IPLib\\', $monorepoVendor . '/mlocati/ip-lib/src'],
    ['Symfony\\Contracts\\EventDispatcher\\', $monorepoVendor . '/symfony/event-dispatcher-contracts'],
    ['Symfony\\Contracts\\Service\\', $monorepoVendor . '/symfony/service-contracts'],
    ['Symfony\\Contracts\\Translation\\', $monorepoVendor . '/symfony/translation-contracts'],
    ['Symfony\\Component\\Console\\', $monorepoVendor . '/symfony/console'],
    ['Symfony\\Component\\EventDispatcher\\', $monorepoVendor . '/symfony/event-dispatcher'],
    ['Symfony\\Component\\Filesystem\\', $monorepoVendor . '/symfony/filesystem'],
    ['Symfony\\Component\\Process\\', $monorepoVendor . '/symfony/process'],
    ['Symfony\\Component\\Runtime\\', $monorepoVendor . '/symfony/runtime'],
    ['Symfony\\Runtime\\Symfony\\Component\\', $monorepoVendor . '/symfony/runtime/Internal'],
    ['Symfony\\Component\\String\\', $monorepoVendor . '/symfony/string'],
    ['Symfony\\Component\\Uid\\', $monorepoVendor . '/symfony/uid'],
    ['Symfony\\Component\\VarDumper\\', $monorepoVendor . '/symfony/var-dumper'],
    ['Symfony\\Polyfill\\Ctype\\', $monorepoVendor . '/symfony/polyfill-ctype'],
    ['Symfony\\Polyfill\\Intl\\Grapheme\\', $monorepoVendor . '/symfony/polyfill-intl-grapheme'],
    ['Symfony\\Polyfill\\Intl\\Idn\\', $monorepoVendor . '/symfony/polyfill-intl-idn'],
    ['Symfony\\Polyfill\\Intl\\Normalizer\\', $monorepoVendor . '/symfony/polyfill-intl-normalizer'],
    ['Symfony\\Polyfill\\Mbstring\\', $monorepoVendor . '/symfony/polyfill-mbstring'],
    ['Symfony\\Polyfill\\Php80\\', $monorepoVendor . '/symfony/polyfill-php80'],
    ['Symfony\\Polyfill\\Php81\\', $monorepoVendor . '/symfony/polyfill-php81'],
    ['Symfony\\Polyfill\\Php84\\', $monorepoVendor . '/symfony/polyfill-php84'],
    ['Symfony\\Polyfill\\Php85\\', $monorepoVendor . '/symfony/polyfill-php85'],
    ['Symfony\\Polyfill\\Php86\\', $monorepoVendor . '/symfony/polyfill-php86'],
    ['Symfony\\Polyfill\\Uuid\\', $monorepoVendor . '/symfony/polyfill-uuid'],
];

$eagerFiles = [
    'functions.php' => $doryRuntime . '/src/functions.php',
    'vendor/ralouphie/getallheaders/src/getallheaders.php' => $monorepoVendor . '/ralouphie/getallheaders/src/getallheaders.php',
    'vendor/nikic/fast-route/src/functions.php' => $monorepoVendor . '/nikic/fast-route/src/functions.php',
    'vendor/symfony/deprecation-contracts/function.php' => $monorepoVendor . '/symfony/deprecation-contracts/function.php',
    'vendor/symfony/polyfill-ctype/bootstrap.php' => $monorepoVendor . '/symfony/polyfill-ctype/bootstrap.php',
    'vendor/symfony/polyfill-ctype/bootstrap80.php' => $monorepoVendor . '/symfony/polyfill-ctype/bootstrap80.php',
    'vendor/symfony/polyfill-intl-grapheme/bootstrap.php' => $monorepoVendor . '/symfony/polyfill-intl-grapheme/bootstrap.php',
    'vendor/symfony/polyfill-intl-grapheme/bootstrap80.php' => $monorepoVendor . '/symfony/polyfill-intl-grapheme/bootstrap80.php',
    'vendor/symfony/polyfill-intl-idn/bootstrap.php' => $monorepoVendor . '/symfony/polyfill-intl-idn/bootstrap.php',
    'vendor/symfony/polyfill-intl-idn/bootstrap80.php' => $monorepoVendor . '/symfony/polyfill-intl-idn/bootstrap80.php',
    'vendor/symfony/polyfill-intl-normalizer/bootstrap.php' => $monorepoVendor . '/symfony/polyfill-intl-normalizer/bootstrap.php',
    'vendor/symfony/polyfill-intl-normalizer/bootstrap80.php' => $monorepoVendor . '/symfony/polyfill-intl-normalizer/bootstrap80.php',
    'vendor/symfony/polyfill-mbstring/bootstrap.php' => $monorepoVendor . '/symfony/polyfill-mbstring/bootstrap.php',
    'vendor/symfony/polyfill-mbstring/bootstrap80.php' => $monorepoVendor . '/symfony/polyfill-mbstring/bootstrap80.php',
    'vendor/symfony/polyfill-php80/bootstrap.php' => $monorepoVendor . '/symfony/polyfill-php80/bootstrap.php',
    'vendor/symfony/polyfill-php81/bootstrap.php' => $monorepoVendor . '/symfony/polyfill-php81/bootstrap.php',
    'vendor/symfony/polyfill-php84/bootstrap.php' => $monorepoVendor . '/symfony/polyfill-php84/bootstrap.php',
    'vendor/symfony/polyfill-php84/bootstrap82.php' => $monorepoVendor . '/symfony/polyfill-php84/bootstrap82.php',
    'vendor/symfony/polyfill-php85/bootstrap.php' => $monorepoVendor . '/symfony/polyfill-php85/bootstrap.php',
    'vendor/symfony/polyfill-php85/bootstrap80.php' => $monorepoVendor . '/symfony/polyfill-php85/bootstrap80.php',
    'vendor/symfony/polyfill-php86/bootstrap.php' => $monorepoVendor . '/symfony/polyfill-php86/bootstrap.php',
    'vendor/symfony/polyfill-php86/bootstrap80.php' => $monorepoVendor . '/symfony/polyfill-php86/bootstrap80.php',
    'vendor/symfony/polyfill-uuid/bootstrap.php' => $monorepoVendor . '/symfony/polyfill-uuid/bootstrap.php',
    'vendor/symfony/polyfill-uuid/bootstrap80.php' => $monorepoVendor . '/symfony/polyfill-uuid/bootstrap80.php',
    'vendor/symfony/string/Resources/functions.php' => $monorepoVendor . '/symfony/string/Resources/functions.php',
    'vendor/symfony/var-dumper/Resources/functions/dump.php' => $monorepoVendor . '/symfony/var-dumper/Resources/functions/dump.php',
];

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

        if (isset($eagerFileRealPaths[$realPath])) {
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
