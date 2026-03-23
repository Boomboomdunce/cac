#!/usr/bin/env node
'use strict';

const keys = [
  'CCP_SESSION_ID',
  'NODE_OPTIONS',
  'DO_NOT_TRACK',
  'OTEL_SDK_DISABLED',
  'CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC',
  'DISABLE_TELEMETRY',
];

const output = {};
for (const key of keys) {
  output[key] = Object.prototype.hasOwnProperty.call(process.env, key)
    ? process.env[key]
    : null;
}

console.log(JSON.stringify(output));
