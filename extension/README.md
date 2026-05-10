# Scopeball extension

## Manual smoke test (Plan 3 milestone)

1. `yarn build:chrome`
2. Load `dist/chrome/` as an unpacked extension.
3. Visit any dApp, trigger a swap or signature, and observe the service-worker console:
   - `[Scopeball] tx { hostname, chainId, to, data, bypassed }`
   - `[Scopeball] typed-sig { hostname, chainId, primaryType, bypassed }`
   - `[Scopeball] personal-sign { hostname, messageLen, bypassed }`

`bypassed: true` indicates the request was caught by the bypass-check observer, not by the inpage proxy.
