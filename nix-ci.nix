{
  # A check that fails is retried once. If it fails again the rest of its suite
  # would be cancelled, and the load a suite makes would depend on when.
  fail-fast = false;
}
