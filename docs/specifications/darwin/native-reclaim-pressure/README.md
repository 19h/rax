# Same-host deferred reclaim pressure

The prior model always used a10s sampling period and10percent autotrim threshold.
An unchanged native control measured warning dispatch pressure2 with an interval
bracket[0.99999958,1.00000288]s; the model measured[9.9999795,10.000244875]s at the
same observed pressure. These are separate executions,not simultaneous samples.
The retained raw timestamps,timebase and comparison.json define the brackets:
(deadline-after)*numer/denom/1e9 <= period_s <=
(deadline-before)*numer/denom/1e9,provided the kernel timestamp lies between the
measured allocation calls. No host memory pressure is created or changed.

The same-host profile now reads the fixed,read-only host node
kern.memorystatus_vm_pressure_level through the existing bounded sysctl adapter.
It accepts exactly four returned bytes. The embedded profile does not invoke
that reader and keeps the existing normal policy. Unknown/missing levels also
retain that normal policy; this fallback is not proof of actual host pressure.

| Dispatch pressure | Period | Autotrim percent |
|---|---|---|
| 1 normal | 10s | 10 |
| 2 warning or urgent | 1s | 5 |
| 4 critical | 0s | 1 |

The kernel pressure enum0/1/2/3 differs from dispatch values1/2/4. Apple
kern_memorystatus_notify.c maps warning and urgent to dispatch2,and its sysctl
returns that dispatch representation. Do not use enum3 as dispatch critical.
XNU vm_reclaim.c supplies the selected default periods and percentages.
A critical zero period drains current reclaimable bytes before division and
leaves the weighted moving average unchanged. Normal/warning paths retain the
existing WMA and round trimming to the guest page size. Policy selection,
conversion and sampling use O(1) time/space plus the existing reclaim walk.
ARM64 ticks = floor(period_ns *24,000,000/1,000,000,000); x86-64 ticks are ns.

Six focused tests cover pressure selection,unavailable/malformed enum values,
closed-profile non-invocation,0/1/10s clocks,zero-period accounting with prior
samples,and5percent versus10percent trimming. Native Darwin integration and
all3host compile/library/CAPI/owning gates are recorded after completion.
The original reclaim fixture is retained for native-versus-emulated comparison.

The retained Apple tag is xnu-12377.121.6,not a claim that the actual Darwin27.2
kernel has that exact source revision. Native controls confirm the observed
normal/warning defaults. Active-swapping override,custom kernel tunables and
asynchronous host-driven guest-ring walking remain unknown or unimplemented;
the latter is not introduced by this sampled-policy fix. Read assumption and
bounded-finding records before extending these claims. The full process goal
remains active.
