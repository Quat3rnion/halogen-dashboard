# The Collector is the only reader of Halogen /metrics

Halogen calculates two throughput gauges (`llamacpp:prompt_tokens_seconds` and `llamacpp:predicted_tokens_seconds`) over the time since the last read of `/metrics`. Each read starts a new window. Halogen also writes float counters with 6 significant digits, so the seconds counters become less precise than one second after 100,000 s of total time. The Collector calculates exact time changes from the windowed gauges and the exact integer token counters. This is correct only when no other client reads `/metrics`. Thus the Collector is the only reader of Halogen, and browsers get data only from the Collector.

## Considered options

- Each browser reads Halogen. Each browser tab starts new gauge windows, and the page needs CORS or a proxy.
- Prometheus reads Halogen. The resolution is the Prometheus scrape interval, and Prometheus also starts new gauge windows.

## Consequences

- Do not add a Prometheus job for Halogen. Prometheus must read a re-export from the Collector.
- A manual `curl` of Halogen `/metrics` makes one Sample incorrect. Use the Collector to examine Halogen.
