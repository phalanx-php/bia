<?php

declare(strict_types=1);

$matrix = require dirname(__DIR__) . '/embedded/matrix.php';
$symbols = $matrix['smoke_symbols'] ?? [];
$classes = $symbols['classes'] ?? [];
$functions = $symbols['functions'] ?? [];

if (!is_array($classes) || !is_array($functions)) {
    fwrite(STDERR, "Invalid Bia embed smoke-symbol matrix.\n");
    exit(1);
}

$classList = var_export(array_values($classes), true);
$functionList = var_export(array_values($functions), true);

echo <<<PHP
foreach ({$classList} as \$symbol) {
    if (!class_exists(\$symbol) && !interface_exists(\$symbol)) {
        throw new RuntimeException("missing " . \$symbol);
    }
}

foreach ({$functionList} as \$symbol) {
    if (!function_exists(\$symbol)) {
        throw new RuntimeException("missing " . \$symbol);
    }
}

PHP;
