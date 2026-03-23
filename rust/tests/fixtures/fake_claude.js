#!/usr/bin/env node
'use strict';

const keys = [
  'CCP_SESSION_ID',
  'CCP_RUNTIME_HOOK',
  'NODE_OPTIONS',
  'DO_NOT_TRACK',
  'OTEL_SDK_DISABLED',
  'CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC',
  'DISABLE_TELEMETRY',
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

console.log(JSON.stringify(output));
