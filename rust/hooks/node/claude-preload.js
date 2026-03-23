'use strict';

(function installClaudePreload() {
  if (globalThis.__ccpClaudePreloadInstalled) {
    return;
  }
  Object.defineProperty(globalThis, '__ccpClaudePreloadInstalled', {
    value: true,
    configurable: false,
    enumerable: false,
    writable: false,
  });

  const dns = require('dns');
  const net = require('net');
  const tls = require('tls');
  const fs = require('fs');

  const blockedDomains = new Set([
    'statsig.anthropic.com',
    'sentry.io',
    'o1137031.ingest.sentry.io',
  ]);

  function normalizeHost(hostname) {
    if (!hostname || typeof hostname !== 'string') {
      return '';
    }
    return hostname.toLowerCase().replace(/\.$/, '');
  }

  function isBlocked(hostname) {
    const normalized = normalizeHost(hostname);
    if (!normalized) {
      return false;
    }
    if (blockedDomains.has(normalized)) {
      return true;
    }

    const labels = normalized.split('.');
    for (let i = 1; i < labels.length - 1; i += 1) {
      if (blockedDomains.has(labels.slice(i).join('.'))) {
        return true;
      }
    }
    return false;
  }

  function blockedError(hostname, syscall) {
    const err = new Error(`connect ECONNREFUSED (blocked by ccp): ${hostname}`);
    err.code = 'ECONNREFUSED';
    err.errno = -111;
    err.hostname = hostname;
    err.syscall = syscall;
    return err;
  }

  function patchDns() {
    const originalLookup = dns.lookup;
    dns.lookup = function ccpLookup(hostname, options, callback) {
      if (typeof options === 'function') {
        callback = options;
        options = {};
      }
      if (isBlocked(hostname)) {
        const err = blockedError(hostname, 'getaddrinfo');
        if (typeof callback === 'function') {
          process.nextTick(() => callback(err));
        }
        return {};
      }
      return originalLookup.call(dns, hostname, options, callback);
    };

    ['resolve', 'resolve4', 'resolve6'].forEach((method) => {
      const original = dns[method];
      if (!original) {
        return;
      }
      dns[method] = function ccpResolve(hostname, ...rest) {
        const callback = rest[rest.length - 1];
        if (isBlocked(hostname)) {
          const err = blockedError(hostname, 'query');
          if (typeof callback === 'function') {
            process.nextTick(() => callback(err));
          }
          return;
        }
        return original.call(dns, hostname, ...rest);
      };
    });

    if (!dns.promises) {
      return;
    }

    const promisesLookup = dns.promises.lookup;
    if (promisesLookup) {
      dns.promises.lookup = function ccpPromiseLookup(hostname, options) {
        if (isBlocked(hostname)) {
          return Promise.reject(blockedError(hostname, 'getaddrinfo'));
        }
        return promisesLookup.call(dns.promises, hostname, options);
      };
    }

    ['resolve', 'resolve4', 'resolve6'].forEach((method) => {
      const original = dns.promises[method];
      if (!original) {
        return;
      }
      dns.promises[method] = function ccpPromiseResolve(hostname, ...rest) {
        if (isBlocked(hostname)) {
          return Promise.reject(blockedError(hostname, 'query'));
        }
        return original.call(dns.promises, hostname, ...rest);
      };
    });
  }

  function hostFromNetArgs(args) {
    if (!args.length) {
      return '';
    }
    if (typeof args[0] === 'object' && args[0] !== null) {
      return args[0].host || args[0].hostname || '';
    }
    if (typeof args[1] === 'string') {
      return args[1];
    }
    return '';
  }

  function blockedSocket(hostname) {
    const socket = new net.Socket();
    process.nextTick(() => {
      socket.destroy(blockedError(hostname, 'connect'));
    });
    return socket;
  }

  function patchNet() {
    const originalConnect = net.connect;
    const originalCreateConnection = net.createConnection;

    net.connect = function ccpNetConnect(...args) {
      const hostname = hostFromNetArgs(args);
      if (isBlocked(hostname)) {
        return blockedSocket(hostname);
      }
      return originalConnect.apply(net, args);
    };

    net.createConnection = function ccpNetCreateConnection(...args) {
      const hostname = hostFromNetArgs(args);
      if (isBlocked(hostname)) {
        return blockedSocket(hostname);
      }
      return originalCreateConnection.apply(net, args);
    };
  }

  function readOptionalFile(path) {
    if (!path) {
      return null;
    }
    try {
      return fs.readFileSync(path);
    } catch (_) {
      return null;
    }
  }

  function normalizeTlsConnectArgs(args) {
    if (typeof args[0] === 'object' && args[0] !== null) {
      return {
        options: { ...args[0] },
        callback: typeof args[1] === 'function' ? args[1] : undefined,
      };
    }

    const options = {};
    options.port = args[0];
    if (typeof args[1] === 'string') {
      options.host = args[1];
    }
    if (typeof args[2] === 'object' && args[2] !== null) {
      Object.assign(options, args[2]);
    }

    return {
      options,
      callback: typeof args[args.length - 1] === 'function' ? args[args.length - 1] : undefined,
    };
  }

  function patchTls() {
    const proxyHostPort = process.env.CCP_PROXY_HOST || process.env.CAC_PROXY_HOST || '';
    const cert = readOptionalFile(process.env.CCP_MTLS_CERT || process.env.CAC_MTLS_CERT);
    const key = readOptionalFile(process.env.CCP_MTLS_KEY || process.env.CAC_MTLS_KEY);
    const ca = readOptionalFile(process.env.CCP_MTLS_CA || process.env.CAC_MTLS_CA);

    if (!proxyHostPort || !cert || !key) {
      return;
    }

    const [proxyHost, proxyPortRaw] = proxyHostPort.split(':', 2);
    const proxyPort = Number(proxyPortRaw || 0);
    const originalTlsConnect = tls.connect;

    tls.connect = function ccpTlsConnect(...args) {
      const { options, callback } = normalizeTlsConnectArgs(args);
      const targetHost = options.host || options.hostname || '';
      const targetPort = Number(options.port || 0);
      const matchesProxy =
        targetHost === proxyHost && (proxyPort === 0 || proxyPort === targetPort);

      if (matchesProxy && !options.cert && !options.key) {
        options.cert = cert;
        options.key = key;
        if (ca) {
          options.ca = options.ca ? [].concat(options.ca, ca) : [ca];
        }
      }

      if (callback) {
        return originalTlsConnect.call(tls, options, callback);
      }
      return originalTlsConnect.call(tls, options);
    };
  }

  function hostFromFetchInput(input) {
    try {
      if (typeof input === 'string') {
        return new URL(input).hostname;
      }
      if (typeof URL !== 'undefined' && input instanceof URL) {
        return input.hostname;
      }
      if (input && typeof input.url === 'string') {
        return new URL(input.url).hostname;
      }
    } catch (_) {
      return '';
    }
    return '';
  }

  function patchFetch() {
    if (typeof globalThis.fetch !== 'function') {
      return;
    }

    const originalFetch = globalThis.fetch.bind(globalThis);
    globalThis.fetch = function ccpFetch(input, init) {
      const hostname = hostFromFetchInput(input);
      if (isBlocked(hostname)) {
        return Promise.reject(blockedError(hostname, 'fetch'));
      }
      return originalFetch(input, init);
    };
  }

  patchDns();
  patchNet();
  patchTls();
  patchFetch();
})();
