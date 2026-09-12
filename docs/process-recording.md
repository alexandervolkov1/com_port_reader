# Process recording

On startup the application creates a dated SQLite database below `processes/` in the application directory. The recorder has its own writer thread, so file I/O does not run on the GUI, acquisition, or control threads.

The database records the session, loaded configuration source, logs, requested/applied/failed actions, raw and derived measurements, and controller output records. The stable tables are `session`, `configurations`, `logs`, `actions`, `measurements`, and `control_outputs`; `process_timeline` is a chronological view across log and action events.

Example inspection query:

```sql
SELECT timestamp, series_name, value
FROM measurements
ORDER BY timestamp;
```

Recorder errors are reported through the application log. The runtime deliberately continues acquisition and control if SQLite initialization or a later write fails; process history is valuable but must not become a control-path dependency. The active database path is reported by the application when recording is available.
