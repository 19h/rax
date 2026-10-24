# GUI expected-version conversion

Native conversion derives the client expected-version word from the executable
PE subsystem version. Independent controls vary the PE OS and subsystem fields;
the observed word follows the subsystem fields. Additional accepted PE images
exercise minor versions above 255.

| PE subsystem major | PE subsystem minor | Observed client word in main and worker |
| ---: | ---: | --- |
| 10 | 0 | `0x0A00` |
| 6 | 2 | `0x0602` |
| 6 | 255 | `0x06FF` |
| 7 | 256 | `0x0700` |
| 8 | 256 | `0x0800` |
| 7 | 65,535 | `0x07FF` |

The measured conversion is `(major << 8) | (minor & 0xFF)` for these accepted
headers. Using the complete minor field was incorrect: subsystem 8.256 would
produce `0x0900` rather than the native `0x0800`. The kernel-model correction
masks the minor field, and its shared regression checks both ARM64 and x64
guest contexts. These observations do not establish loader acceptance or
conversion for every possible major-version value.

`observations.json` contains seven accepted native controls, including the
independent OS/subsystem variations, and binds each original private log by byte
count and SHA-256. Raw logs include memory buffers and remain private; the
published projection retains header values, observed client words and
before/after executable hashes. The source executable is bound to native
callback controller 5, which compiled callback source 4; attribution follows
the original receipt rather than the controller's numeric suffix.

All controls ran in native ARM64 processes on the same observed Windows kernel.
The source executable remained unchanged. This establishes the recorded field
conversion, not independent x64-kernel behavior or complete GUI initialization.
