# Commonplace CLI

The `commonplace` command-line tool for the Commonplace REST API, and the tests that prove it works.

## Language

### Testing

**Fake host**:
A stand-in for Commonplace or Auth that a test starts inside its own process and scripts request by request. It is used by every test that runs on every change.
_Avoid_: Mock server, stub
