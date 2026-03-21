# Error Quality Tests

These tests verify that error messages are clear and point to the right
source location. They are implementation-specific (other KS interpreters
may format errors differently) and are NOT part of the conformance suite.

Each `.ks` file produces an error. The `.expected_err` file contains a
substring that must appear in stderr. These are manually verified for
ariadne rendering quality — the automated check is just substring matching.
