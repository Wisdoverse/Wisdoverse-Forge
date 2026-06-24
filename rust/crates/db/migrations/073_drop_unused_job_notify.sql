-- Drop the unused job_queue NOTIFY trigger (#892, finding F045).
--
-- `notify_new_job()` fired `pg_notify('new_job', NEW.queue)` on every job_queue
-- INSERT, but nothing ever LISTENed on the channel — every queue worker (the
-- generic `Worker`, plus the bespoke self_fix_pr / project_clone loops) polls.
-- The trigger was pure per-insert overhead, and its presence implied a
-- low-latency wake-up path that does not exist, inviting contributors to tune
-- poll intervals up on a false assumption.
--
-- Drop both objects. If a notify-driven path is wired later (a `PgListener`
-- arm in the worker loop), a forward migration can reintroduce them. Idempotent
-- so it is safe on fresh databases and on production where the trigger exists.
DROP TRIGGER IF EXISTS job_queue_notify ON job_queue;
DROP FUNCTION IF EXISTS notify_new_job();
