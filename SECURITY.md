# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |
| < 0.1   | :x:                |

## Reporting a Vulnerability

We take security vulnerabilities seriously. If you discover a security issue, please report it responsibly.

### How to Report

1. **Do NOT** open a public GitHub issue for security vulnerabilities
2. Use GitHub private vulnerability reporting for this repository, or contact
   the maintainers through the repository's published security channel
3. Include the following information:
   - Type of vulnerability
   - Full path to the vulnerable file(s)
   - Steps to reproduce
   - Potential impact
   - Suggested fix (if any)

### What to Expect

- **Acknowledgment**: Within 48 hours of your report
- **Initial Assessment**: Within 5 business days
- **Resolution Timeline**: Depends on severity
  - Critical: 24-48 hours
  - High: 7 days
  - Medium: 30 days
  - Low: Next release cycle

### Disclosure Policy

- We will coordinate disclosure with you
- Credit will be given to reporters (unless anonymity is requested)
- Please allow us reasonable time to fix issues before public disclosure

## Security Measures

### Authentication

- JWT-based authentication with short-lived access tokens
- Refresh token rotation
- Secure token storage recommendations in documentation

### Data Protection

- Authenticated API routes are protected by Rust/Axum middleware; intentionally
  public infrastructure endpoints such as `/health` are documented separately
- Input validation at API boundaries and domain services
- Output sanitization to prevent XSS
- SQL injection prevention via parameterized queries

### API Security

- Auth guards on all agent management, voice, and plugin endpoints
- WebSocket origin validation with configurable allowlist
- Message size limits and rate limiting on WebSocket connections
- OpenTelemetry distributed tracing for security audit trail

### Network Security

- HTTPS recommended for production
- CORS configuration with explicit origins
- Rate limiting on API endpoints
- WebSocket connection authentication

### Infrastructure

- Environment variable management for secrets
- No hardcoded credentials in codebase
- Docker images scanned for vulnerabilities
- Regular dependency updates

## Best Practices for Users

### Production Deployment

1. **Use HTTPS**: Always deploy behind HTTPS in production
2. **Environment Variables**: Never commit secrets to version control
3. **Database Security**: Use strong passwords, restrict network access
4. **Regular Updates**: Keep Wisdoverse Forge and dependencies updated
5. **Access Control**: Limit who can access the Wisdoverse Forge interface

### Configuration

```bash
# Required security environment variables
JWT_SECRET=<strong-random-secret>        # At least 32 characters
SESSION_SECRET=<strong-random-secret>    # At least 32 characters

# Recommended settings
ENVIRONMENT=production                    # Enables production security checks
CORS_ORIGIN=https://your-domain.com       # Restrict browser origins
RATE_LIMIT_MAX=100                        # Requests per window
```

### Network Isolation

- Run Wisdoverse Forge on internal network when possible
- Use firewall rules to restrict access
- Consider VPN for remote access

## Known Security Considerations

### Agent Container Isolation

Wisdoverse Forge runs AI agents in isolated Docker containers. This is a powerful feature that should be:

- Used only in trusted environments
- Protected by network-level security
- Monitored for unexpected activity

### File Access

Container CLIs such as Claude Code can access files in working directories. Ensure:

- Working directories don't contain sensitive files
- Proper file permissions are set
- Activity is logged and monitored

### WebSocket Connections

WebSocket connections maintain persistent state. Consider:

- Connection timeouts for idle sessions
- Rate limiting on message frequency
- Authentication validation on each connection

## Security Audit

For detailed security findings and recommendations, see:

- [docs/guides/deployment.md](docs/guides/deployment.md) - Deployment guide
- [docs/security/dependency-policy.md](docs/security/dependency-policy.md) - Dependency security policy

## Responsible Disclosure

We appreciate the security research community's efforts. We commit to:

- Not pursuing legal action against good-faith researchers
- Working with researchers to understand and resolve issues
- Publicly acknowledging contributions (with permission)

Thank you for helping keep Wisdoverse Forge secure!
