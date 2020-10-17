Test file: "private" URLs (should all be excluded when using `-E` flag).

- Loopback: http://127.0.0.1
- Link-local 1: http://169.254.0.1
- Link-local 2: https://169.254.10.1:8080
- Private class A: http://10.0.1.1
- Private class B: http://172.16.42.42
- Private class C: http://192.168.10.1

IPv6:

- Loopback: http://[::1]
