# Security Policy Summary

## Vulnerability Reporting
The rodbus library uses GitHub's private vulnerability reporting system. Users should avoid public issues and instead access the repository's Security tab to "Report a vulnerability" and submit private reports to maintainers.

## Response Timeline
The maintainers aim to acknowledge reports within 48 hours, provide initial feedback within 5 business days, and typically resolve confirmed issues within 90 days. These targets are non-binding community norms, not service guarantees.

## Staying Updated
Users can monitor security developments by watching the repository with custom settings for releases and security alerts, subscribing to GitHub Security Advisories RSS feeds, or using tools like `cargo audit`.

## Supply Chain Protections
The project implements several security measures:
- **Automated scanning**: Dependencies undergo nightly audits using `cargo audit`
- **PR verification**: All pull requests are checked for vulnerabilities
- **Dependency strategy**: The team minimizes external dependencies and maintains committed lock files for reproducible builds
- **Release audits**: Security checks occur during CI/CD processes

## User Recommendations
Developers should install and run `cargo audit` locally and integrate it into their CI/CD pipelines for ongoing vulnerability monitoring.


