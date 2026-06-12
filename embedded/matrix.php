<?php

declare(strict_types=1);

return [
    'modules' => [
        'Bia' => 'embedded',
        'Bootstrap' => 'embedded',
        'Engine' => 'embedded',
        'Err' => 'embedded',
        'Invocation' => 'embedded',
        'Mark' => 'embedded',
        'Schema' => 'embedded',
        'Scope' => 'embedded',
        'Supervision' => 'embedded',
    ],
    'packages' => [
        ['namespace' => 'Phalanx\\', 'path' => 'phalanx/src', 'package' => 'phalanx-php/phalanx'],
        ['namespace' => 'Phalanx\\Bia\\', 'path' => 'libs/bia-runtime/src', 'package' => 'phalanx-php/bia-runtime'],
    ],
    'eager_files' => [
        'functions.php' => 'libs/bia-runtime/src/functions.php',
    ],
    'smoke_symbols' => [
        'classes' => [
            'Phalanx\\Phalanx',
            'Phalanx\\Mark\\Mark',
            'Phalanx\\Bia\\Runtime\\BiaCli',
            'Phalanx\\Bia\\Runtime\\Host\\HostFacts',
        ],
        'functions' => [
            'dump',
        ],
    ],
];
