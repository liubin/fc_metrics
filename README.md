## fc-metrics-generator

`fc-metrics-generator` is used to convert from Firecracker's metrics.rs to golang version of metrics.

```
update-generated-fc-metrics https://raw.githubusercontent.com/firecracker-microvm/firecracker/master/src/logger/src/metrics.rs
```

The generated golang source codes will be saved to file `src/runtime/virtcontainers/fc_metrics.go`.
