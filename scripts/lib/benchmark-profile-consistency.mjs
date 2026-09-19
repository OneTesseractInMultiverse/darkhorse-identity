// Completed HTTP checks must not disappear from adapter/SQL measurements.
export function verifyPhaseProfile(profile, summary) {
  const { total, pool_acquire, decision } = profile.rust.stages;
  const completed = summary.outcomes.authorized + summary.outcomes.denied;
  if (
    total.count < completed ||
    total.count > summary.attempts - summary.outcomes.healthy ||
    pool_acquire.count !== total.count ||
    decision.count < summary.outcomes.authorized ||
    profile.sql.groups.reduce((sum, group) => sum + group.calls, 0) < completed
  )
    throw new Error("Incomplete benchmark profiling for completed requests.");
}
