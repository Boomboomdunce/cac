#!/usr/bin/env node
'use strict';

const { execFileSync } = require('child_process');

const keys = [
  'CCP_SESSION_ID',
  'CCP_RUNTIME_HOOK',
  'CLAUDE_CONFIG_DIR',
  'NODE_OPTIONS',
  'DO_NOT_TRACK',
  'OTEL_SDK_DISABLED',
  'CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC',
  'DISABLE_TELEMETRY',
  'HTTPS_PROXY',
  'HTTP_PROXY',
  'ALL_PROXY',
  'NO_PROXY',
  'CCP_MTLS_CERT',
  'CCP_MTLS_KEY',
  'CCP_MTLS_CA',
  'CAC_MTLS_CERT',
  'CAC_MTLS_KEY',
  'CAC_MTLS_CA',
  'CCP_PROXY_HOST',
  'CAC_PROXY_HOST',
  'NODE_EXTRA_CA_CERTS',
  'HOSTALIASES',
  'HOSTNAME',
  'COMPUTERNAME',
  'TZ',
  'LANG',
  'ANTHROPIC_BASE_URL',
  'ANTHROPIC_AUTH_TOKEN',
  'ANTHROPIC_API_KEY',
];

const output = {};
for (const key of keys) {
  output[key] = Object.prototype.hasOwnProperty.call(process.env, key)
    ? process.env[key]
    : null;
}

function readCommand(command, args) {
  try {
    return execFileSync(command, args, { encoding: 'utf8' }).trim();
  } catch (_) {
    return null;
  }
}

output.hostnameCommand = readCommand('hostname', []);
if (process.platform === 'win32') {
  output.machineIdCommand = readCommand('reg', [
    'query',
    'HKLM\\SOFTWARE\\Microsoft\\Cryptography',
    '/v',
    'MachineGuid',
  ]);
  output.platformUuidCommand = readCommand('wmic', ['csproduct', 'get', 'UUID']);
  output.macAddressCommand = readCommand('getmac', []);
  output.powershellMachineId = readCommand('powershell', [
    '-NoProfile',
    '-Command',
    "(Get-ItemProperty -Path 'HKLM:\\SOFTWARE\\Microsoft\\Cryptography').MachineGuid",
  ]);
  output.powershellPlatformUuid = readCommand('powershell', [
    '-NoProfile',
    '-Command',
    '(Get-CimInstance Win32_ComputerSystemProduct).UUID',
  ]);
  output.powershellMacAddress = readCommand('powershell', [
    '-NoProfile',
    '-Command',
    '(Get-NetAdapter | Select-Object -First 1 -ExpandProperty MacAddress)',
  ]);
} else {
  output.machineIdCommand = readCommand('cat', ['/etc/machine-id']);
  output.ioregCommand = readCommand('ioreg', ['-rd1', '-c', 'IOPlatformExpertDevice']);
}

console.log(JSON.stringify(output));
