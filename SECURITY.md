# Security Policy

Gatebase controls database access only when database network policy forces clients through Gatebase proxies. Do not leave direct production database access open to human users.

Broker admin APIs require SQLite-backed users and signed bearer tokens. Bootstrap the first `admin` user locally on the broker host, store strong passwords, and expose the broker only through TLS.

Report vulnerabilities privately through the project security contact once configured.
