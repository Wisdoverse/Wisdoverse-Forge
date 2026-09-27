# Migrate MinIO objects to RustFS

This procedure copies attachment objects through the S3 API. MinIO and RustFS
use separate data volumes; do not mount or copy `minio-data` as RustFS data.
The MinIO volume remains the rollback source until the migration is accepted.

## Before you start

- The examples use `agentforge` for both buckets. If your source bucket differs,
  replace `minio/agentforge` with its actual name. Replace `rustfs/agentforge`
  with the destination bucket configured by `S3_BUCKET` throughout the procedure.
- Schedule a short attachment-write freeze for the final sync and cutover.
- Keep the existing MinIO service and `minio-data` volume intact.
- Save the source Compose files, image reference, and configuration in secure
  backup storage so the source can be recreated if needed. Do not use
  `--remove-orphans` or `down --volumes` during migration or rollback retention.
- Configure explicit `S3_ACCESS_KEY` and `S3_SECRET_KEY` for RustFS; the
  service does not inherit legacy `MINIO_*` credentials. Keep the API on its
  existing MinIO configuration during the initial copy. Do not restart or
  recreate the API until cutover: non-empty `S3_*` settings take precedence
  over `MINIO_*` when the API loads its configuration.
- Set `RUSTFS_API_PORT=9002` and `RUSTFS_CONSOLE_PORT=9003` in `docker/.env`
  and keep them there until the old MinIO service is retired. Compose reads
  this file; it does not export its values into your shell.
- Run the already-used `mc` S3-compatible client from the Compose network (or
  use endpoints reachable from the client), and obtain credentials for both
  endpoints. Do not put real credentials in shell history or docs.
- Start only RustFS on temporary host ports so MinIO can remain online:

  ```bash
  docker compose --env-file docker/.env \
      -f docker/compose.yml -f docker/compose.prod.yml \
      -f docker/compose.storage.yml --profile prod --profile storage \
      up -d --wait rustfs
  ```

- Configure aliases with endpoints reachable from `mc`. For a host-run client,
  use `http://127.0.0.1:9002` for RustFS; MinIO's source endpoint depends on
  its current host mapping. The shell running `mc` must have the endpoint and
  credential variables configured separately; `docker/.env` is not sourced.

```bash
mc alias set minio "$MINIO_ENDPOINT" "$MINIO_ACCESS_KEY" "$MINIO_SECRET_KEY"
mc alias set rustfs http://127.0.0.1:9002 "$S3_ACCESS_KEY" "$S3_SECRET_KEY"
```

## Copy and cut over

1. Confirm RustFS readiness at `http://127.0.0.1:9002/health/ready`. Its
   temporary console port is 9003.
2. Copy current objects over the S3 API, preserving object metadata. This copies
   current objects only; it does not migrate object versions or IAM policies.

   ```bash
   mc mb --ignore-existing rustfs/agentforge
   mc mirror --overwrite --preserve minio/agentforge rustfs/agentforge
   ```

3. Compare source and destination inventories for every key. Verify object
   count, key names, size, user metadata, and SHA-256 of the downloaded bytes;
   do not treat matching ETags as SHA-256 proof. Run the checks in Bash with
   `set -e -o pipefail`; a failed `mc stat`, `mc cat`, or hash command must
   stop verification. For each source and destination key, record its metadata
   and digest:

   ```bash
   set -e -o pipefail
   OBJECT='<alias>/<bucket>/<key>'
   mc stat --json "$OBJECT"
   ```

   Then run the matching hash command in the same Bash session. Use this on
   Linux/WSL:

   ```bash
   mc cat "$OBJECT" | sha256sum
   ```

   Use this on macOS:

   ```bash
   mc cat "$OBJECT" | shasum -a 256
   ```

   Repeat for source and destination keys and compare the complete inventories;
   matching ETags alone are not SHA-256 proof.

4. Freeze attachment writes by stopping the Rust API using the source
   deployment's Compose files. This also pauses other API operations. For the
   self-contained production profile:

   ```bash
   docker compose --env-file docker/.env -f docker/compose.yml \
     -f docker/compose.prod.yml --profile prod stop agentforge-server
   ```

   Keep the API stopped until the final sync and comparison pass. Preview the
   final sync and inspect all proposed deletions before running it:

   ```bash
   mc mirror --dry-run --overwrite --remove --preserve minio/agentforge rustfs/agentforge
   ```

   `--remove` deletes destination objects missing from the source. Use it only
   after confirming `rustfs/agentforge` is the dedicated attachment bucket;
   it must not contain unrelated objects. Then run the final sync:

   ```bash
   mc mirror --overwrite --remove --preserve minio/agentforge rustfs/agentforge
   ```

   Compare the complete inventories again. Every key, size, SHA-256, and user
   metadata field must match before ending the write freeze. See the
   [`mc mirror` reference](https://docs.min.io/aistor/reference/cli/mc-mirror/).

5. After the final sync and inventory comparison pass, run `make prod-storage`
   to switch the API to `STORAGE_PROVIDER=s3` and the S3 settings. Confirm
   attachment downloads through the API and a new upload/download.
6. Retain MinIO and its volume until acceptance. Do not delete the source as
   part of cutover.

## Roll back

If RustFS has accepted writes after cutover, freeze writes. Preview the reverse
sync and inspect proposed deletions from the MinIO destination:

```bash
mc mirror --dry-run --overwrite --remove --preserve rustfs/agentforge minio/agentforge
```

Run the reverse sync only after confirming `minio/agentforge` is the dedicated
attachment bucket; `--remove` deletes destination objects absent from RustFS:

```bash
mc mirror --overwrite --remove --preserve rustfs/agentforge minio/agentforge
```

Verify every key, size, metadata field, and SHA-256 before pointing the Rust API
back to MinIO. Because `S3_*` takes precedence, either restore the source MinIO
endpoint and credentials in the corresponding `S3_*` variables, or clear the
`S3_*` values and use the legacy `STORAGE_PROVIDER=minio` and `MINIO_*`
settings. Remove the `compose.storage.yml` override from the Compose command,
then start the API with `make prod` or the original production profile. Resume
writes only after API upload and download checks pass. Never roll back by
swapping data volumes.

This runbook describes a migration procedure; no production data migration or
deployment is implied by following this documentation change.
