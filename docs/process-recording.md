# Process recording

The GUI attempts to create a new SQLite database at application startup under `processes/YYYY-MM-DD/` in the application directory. The timestamped filename is chosen by `new_process_database_path`; the active path is written to the application log. Debug builds use the repository as application directory; release builds use the executable directory.

A recording session lasts for the application process, not one acquisition start/stop interval. Stopping/starting acquisition and reloading profiles continue in the same database. A new launch creates a new session.

## What is stored

| Table | Important columns | Meaning |
| --- | --- | --- |
| `session` | `id` (always 1), `started_at`, `ended_at`, `application_version` | Application-session lifetime. |
| `configurations` | `id`, `timestamp`, `startup_path`, `source` | Loaded profile path and optional source text. |
| `logs` | `id`, `timestamp`, `level`, `message` | Informational/error application messages, including `app.log`. |
| `actions` | `id`, `timestamp`, `origin`, `action_type`, `connection_id`, `series_id`, `series_name`, `details`, `status`, `completed_at`, `result`, `error` | Requested action and its applied/failed outcome. |
| `measurements` | `id`, `timestamp`, `connection_id`, `series_id`, `series_name`, `value` | Successful raw/derived/diagnostic samples. |
| `control_outputs` | `id`, `timestamp`, `loop_name`, `controller_kind`, `input_series_id`, `connection_id`, `setpoint`, `measurement`, `requested_output`, `actual_output`, `unconstrained_output`, `proportional`, `integral`, `derivative`, `saturated` | Controller computation and output result. |

Timestamps are Unix seconds stored as REAL. Connection and series IDs are text columns; action IDs are integer keys. Action statuses are `requested`, `applied` and `failed`. A null actual output means there was no confirmed successful automatic write; consult the error log to distinguish rejection from transport failure. Controller-specific unavailable terms are null.

`process_timeline` combines logs, configuration loads, action requests and their completions into one view. It does not include every measurement. Its columns are `timestamp`, `event_type`, `level`, `action_id`, `origin`, `action_type`, `connection_id`, `series_id`, `series_name`, `message` and `details`.

Names are snapshots at recording time. Renaming a series does not rewrite earlier rows; use `series_id` when following its identity. Display decimation and plot visibility do not disable measurement recording. Manual instrument reads/writes are action results; they are not automatically new periodic measurement samples.

## Queries

Latest measurements:

```sql
SELECT timestamp, series_name, value
FROM measurements
ORDER BY timestamp DESC, id DESC
LIMIT 100;
```

History for one series name:

```sql
SELECT timestamp, value
FROM measurements
WHERE series_name = 'temperature'
ORDER BY timestamp, id;
```

Actions during a Unix-time interval (bind `:start` and `:end` in your SQL client):

```sql
SELECT timestamp, action_type, status, completed_at, result, error
FROM actions
WHERE timestamp >= :start AND timestamp < :end
ORDER BY timestamp, id;
```

Application errors:

```sql
SELECT datetime(timestamp, 'unixepoch') AS utc_time, message
FROM logs
WHERE level = 'error'
ORDER BY timestamp DESC, id DESC;
```

Requested and confirmed output:

```sql
SELECT timestamp, loop_name, requested_output, actual_output
FROM control_outputs
ORDER BY timestamp, id;
```

Configuration and action timeline:

```sql
SELECT timestamp, event_type, action_type, message, details
FROM process_timeline
ORDER BY timestamp;
```

## Threading, failure and shutdown

A dedicated writer thread owns SQLite. Producer threads enqueue records; disk work is not performed in acquisition/control calculations. Application events and scenario action completions are published independently of the database sink. Losing recording therefore does not disable scenario scheduling.

SQLite uses WAL and `synchronous=NORMAL`. The first writer failure is retained for reporting and disables subsequent writes for that sink. Acquisition/control continue; there is no automatic recovery of missing records. Fix permissions/disk space and start a new application session to restore recording.

The final recorder owner signals the writer and joins it after queued records have been handled; the writer flushes if it is still available. SQLite destruction records session end on the normal path. A crash or write failure can leave `ended_at` absent or history incomplete. WAL/NORMAL is not a guarantee that the very latest records survive power loss.

The application log also has text files under `logs`. Database creation or recording-disabled errors should be investigated there if no database is available. Keep the SQLite WAL/SHM companions together when examining an open session, or close the app before copying the database.
