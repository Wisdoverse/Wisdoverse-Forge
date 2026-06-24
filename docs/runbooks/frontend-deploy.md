# Frontend Deploy (Staging)

`/opt/agentforge/www` is a symlink to the reverse proxy web root. Deploy via the
symlink so permissions stay consistent with the proxy user.

## Command

```bash
npx vite build
cp public/* dist/
docker run --rm \
  -v "$(pwd)/dist:/src" \
  -v /opt/agentforge/www:/dst \
  alpine sh -c "rm -rf /dst/assets && cp -r /src/* /dst/ && chown -R 1000:1000 /dst"
```

## Notes

- Use docker for the copy to handle UID/GID differences — avoids `sudo`.
- `chown 1000:1000` matches the proxy container's app user.
- `rm -rf /dst/assets` ensures stale hashed bundles are removed before copying new ones.
