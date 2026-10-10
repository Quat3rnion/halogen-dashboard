# Halogen Dashboard

A live dashboard that shows the health and load of the Halogen inference server on frame.

## Language

### Sources

**Halogen**:
The LLM inference server on frame that this dashboard monitors.
_Avoid_: server, backend, llama

**Slot**:
A fixed position in Halogen that runs one request at a time. Halogen has a fixed number of Slots.
_Avoid_: worker, lane, thread

**Collector**:
The one process that reads data from Halogen. Browsers get data from the Collector, not from Halogen.
_Avoid_: scraper, poller, exporter, agent

**Scrape**:
One read of a Halogen status surface by the Collector.
_Avoid_: poll, fetch, pull

**Sample**:
The values from one Scrape, with the time of that Scrape.
_Avoid_: datapoint, tick, snapshot, reading

**Rollup**:
The minimum, mean and maximum of one value over one minute of Samples.
_Avoid_: aggregate, bucket, downsample

### Availability

**Outage**:
A period when Scrapes of Halogen fail. The Collector stores no values for an Outage.
_Avoid_: downtime, gap, offline

**Restart**:
The point where a Halogen counter goes down, which shows that Halogen started again.
_Avoid_: reset, reboot, crash

**Uptime**:
The time since the last Restart or the end of the last Outage, whichever is later.
_Avoid_: availability, alive time

**Status**:
The health of Halogen at one time. The Status is **Up**, **Degraded** or **Down**.
_Avoid_: state, health, condition

**Up**:
The Status when Scrapes work and Halogen, its engine and its NPU all report ok.
_Avoid_: online, healthy

**Degraded**:
The Status when Scrapes work, but the engine or the NPU reports a fault.
_Avoid_: partial outage, warning, unhealthy

**Down**:
The Status during an Outage.
_Avoid_: offline, dead

**Saturation**:
High load where all Slots are busy and requests are queued, or the KV pool is almost full. Saturation is not a Status.
_Avoid_: overload, congestion, busy

### Rates

**Generation speed**:
Generated tokens per second of generation time. It shows how fast one request feels.
_Avoid_: tok/s, TPS, decode speed

**Throughput**:
Generated tokens per wall-clock second across all Slots. It shows the total load on Halogen.
_Avoid_: tok/s, TPS, bandwidth

**Prefill speed**:
Prompt tokens per second of prefill time.
_Avoid_: prompt tok/s, PP speed

**Prefill throughput**:
Prompt tokens per wall-clock second across all Slots.
_Avoid_: prompt tok/s

**Mean service time**:
The mean prefill and generation time of one completed request. It does not include the time that the request waits as a **Queued request**.
_Avoid_: latency, response time

### Load and cache

**Busy Slot**:
A Slot that runs a request now.
_Avoid_: in-flight, processing

**Queued request**:
A request that waits for a free Slot.
_Avoid_: deferred request, waiting request, backlog

**KV pool**:
The shared store of KV positions that all Slots in Halogen use.
_Avoid_: KV cache, context memory

**KV pool use**:
The part of the KV pool that Halogen holds now, as a share of the full KV pool.
_Avoid_: KV cache usage, KV usage ratio

**Prompt cache**:
The Halogen feature that reuses earlier prompt prefixes, so that Halogen does not process those tokens again.
_Avoid_: KV cache, cache

**Prompt-cache hit rate**:
The share of prompt tokens that the Prompt cache covered.
_Avoid_: cache hit rate, token hit rate

**Draft acceptance**:
The share of draft tokens that Halogen accepts during speculative decoding.
_Avoid_: speculative hit rate, acceptance ratio

## Flagged ambiguities

- "tok/s" can mean **Generation speed** or **Throughput**. The two values are different when more than one Slot is busy, and when Halogen is idle. Always use one of the two names.
- "cache" alone can mean the **KV pool** or the **Prompt cache**. Halogen metric names use "kv_cache" for the KV pool. Always say which one.
- The Halogen /cache surface shows hit rates since Halogen started. The **Prompt-cache hit rate** on the dashboard covers only the selected time range.

## Example dialogue

> **Dev:** "Can the browser read Halogen directly to save a hop?"
> **Domain expert:** "No. Only the **Collector** does a **Scrape**. Each browser gets **Samples** from the Collector."
> **Dev:** "Four requests run at 40 tokens per second each. What is the tok/s?"
> **Domain expert:** "Do not say tok/s. **Generation speed** is 40. **Throughput** is about 160."
