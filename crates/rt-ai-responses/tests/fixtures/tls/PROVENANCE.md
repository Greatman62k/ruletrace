# TLS test fixture provenance

`self-signed-cert.der` and `self-signed-key.pkcs8.der` are a self-signed
leaf certificate (subject `CN=ruletrace-test-loopback`, subject alternative
name `IP:127.0.0.1`, ECDSA P-256) and its PKCS#8 private key.

- Generated once, for tests only, with OpenSSL 3.6.5:

  ```
  openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes \
    -not_before 20200101000000Z -not_after 21200101000000Z \
    -subj "/CN=ruletrace-test-loopback" \
    -addext "subjectAltName=IP:127.0.0.1" \
    -addext "basicConstraints=critical,CA:FALSE" \
    -keyout key.pem -out cert.pem
  openssl x509 -in cert.pem -outform DER -out self-signed-cert.der
  openssl pkcs8 -topk8 -nocrypt -in key.pem -outform DER \
    -out self-signed-key.pkcs8.der
  ```

- Validity 2020-01-01 to 2120-01-01, so a wrong system clock cannot turn the
  expected unknown-issuer failure into an expiry failure.
- The private key is public test material. It protects nothing and must never
  be trusted, added to a trust store, or used outside `tls_verification.rs`.
- SHA-256: cert `3b4b13438efd0c27cfa5d9bebacf3c8e5cb6dbbf4884431ae01f2b8e5c40b87e`,
  key `b8e6fabaa764c83929590982294ac603a3535026bcd3fde5831cd29689094531`.
- Replacing these files is a reviewed change: the test asserts the exact
  alert the client sends for an unknown issuer.
