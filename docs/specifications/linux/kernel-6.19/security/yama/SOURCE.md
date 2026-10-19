# Source provenance

- Title: Yama Linux Security Module, `security/yama/yama_lsm.c`.
- Issuer: Linux kernel contributors; author Kees Cook; copyright notices and
  SPDX `GPL-2.0-only` retained in the source.
- Revision: Linux `v6.19`.
- URL: https://raw.githubusercontent.com/torvalds/linux/v6.19/security/yama/yama_lsm.c
- Retrieved: 2026-10-19.
- SHA-256: `146cc6229f8218b37d221a6485def295bd4ebca008ad8ecaec81325d65cab903`.

`yama_task_prctl` defines `PR_SET_PTRACER` validation. The existing RAX procfs
reports `ptrace_scope=0`; this change preserves that setting. Credential and
dumpability checks remain separate from transport reachability.
