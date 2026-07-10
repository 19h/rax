# Test generators

Generator programs live outside `tests/` so generated output, executable test
suites, and generator implementation have distinct ownership.

`arm_structure.py` is the historical handwritten ARM tree generator. Its
required `tests/suites/isa/arm/structure.json` input is not tracked, so it is
retained for provenance but is not currently a complete regeneration command.
