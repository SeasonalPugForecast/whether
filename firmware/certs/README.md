# Root certificates

The firmware verifies each API host against one root, compiled in from this
directory. embedded-tls checks the chain the server sends link by link, starting at
the root, so the root must be the issuer of the **last** certificate in that chain
(not necessarily the root a browser would show). `fw_config::default_root` maps the
hosts; `[tls] roots` in `whether-fw.toml` can override a host with another file
listed here.

| File | Subject | SHA-256 fingerprint | Expires | Hosts |
|---|---|---|---|---|
| `isrg-root-x1.der` | ISRG Root X1 | `96:BC:EC:06:26:49:76:F3:74:60:77:9A:CF:28:C5:A7:CF:E8:A3:C0:AA:E1:1A:8F:FC:EE:05:C0:BD:DF:08:C6` | 2035-06-04 | `*.open-meteo.com` |
| `usertrust-rsa.der` | USERTrust RSA Certification Authority | `E7:93:C9:B0:2F:D8:AA:13:E2:1C:31:22:8A:CC:B0:81:19:64:3B:74:9C:89:89:64:B1:74:6D:46:C3:D4:CB:D2` | 2038-01-18 | `*.openweathermap.org` |
| `harica-rootca-2015.der` | Hellenic Academic and Research Institutions RootCA 2015 | `A0:40:92:9A:02:CE:53:B4:AC:F4:F2:FF:C6:98:1C:E4:49:6F:75:5E:6D:45:FE:0B:2A:69:2B:CD:52:52:3F:36` | 2040-06-30 | `api.met.no` |

## Where they come from (2026-06-28)

Each file was converted from the host's CA bundle:

```bash
openssl x509 -in /etc/ssl/certs/ISRG_Root_X1.pem -outform DER -out isrg-root-x1.der
openssl x509 -in /etc/ssl/certs/USERTrust_RSA_Certification_Authority.pem -outform DER -out usertrust-rsa.der
openssl x509 -in /etc/ssl/certs/Hellenic_Academic_and_Research_Institutions_RootCA_2015.pem -outform DER -out harica-rootca-2015.der
openssl x509 -inform DER -in <file> -noout -fingerprint -sha256
```

All three fingerprints match Mozilla's list of included root certificates (CCADB
`IncludedCACertificateReportCSVFormat`), and `isrg-root-x1.der` is byte-identical
to <https://letsencrypt.org/certs/isrgrootx1.der>.

## The chains the servers sent (2026-06-28)

- `api.open-meteo.com`, `air-quality-api.open-meteo.com`: leaf ← Let's Encrypt YR1
  ← ISRG Root YR (cross-signed by **ISRG Root X1**).
- `api.open-meteo.com` from the other server some resolvers return: an ECDSA chain, leaf (P-256) ← Let's Encrypt YE2
  (P-384) ← ISRG Root YE ← ISRG Root X2 (cross-signed by **ISRG Root X1**). The same
  root verifies it, but it needs embedded-tls's `p384` feature. Which server a
  device reaches depends on its DNS, so a host may send either kind of chain.
- `api.openweathermap.org`: leaf ← Sectigo OV R36 ← Sectigo Root R46 (cross-signed)
  ← **USERTrust RSA** (self-signed, sent too).
- `api.met.no`: leaf ← GEANT TLS RSA 1 ← HARICA TLS RSA Root CA 2021 (cross-signed by
  **Hellenic RootCA 2015**).

When a host changes the chain it sends, its handshake fails with
`InvalidCertificate`; check the new chain with
`openssl s_client -connect <host>:443 -showcerts` and update the map.
