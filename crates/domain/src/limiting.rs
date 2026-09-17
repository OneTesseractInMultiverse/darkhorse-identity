//! Fixed-window attempt budgets from explicit snapshots and authoritative time.
pub const MAX_TIME: u64 = (1 << 53) - 1;
pub const MAX_WINDOW_MS: u32 = 900_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitError {
    InvalidInput,
    UnsafeState,
    ClockRollback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BudgetRule {
    limit: u32,
    window_ms: u32,
}
impl BudgetRule {
    pub fn new(limit: u32, window_ms: u32) -> Result<Self, LimitError> {
        if !(1..=1_000_000).contains(&limit) || !(1000..=MAX_WINDOW_MS).contains(&window_ms) {
            return Err(LimitError::InvalidInput);
        }
        Ok(Self { limit, window_ms })
    }
    pub fn limit(self) -> u32 {
        self.limit
    }
    pub fn window_ms(self) -> u32 {
        self.window_ms
    }
}

/// Caller-owned keyed digest; never construct keys from untrusted raw identity data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    pub key: [u8; 32],
    pub rule: BudgetRule,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attempt {
    budgets: Vec<Budget>,
}
impl Attempt {
    pub fn new(budgets: Vec<Budget>) -> Result<Self, LimitError> {
        if budgets.is_empty()
            || budgets.len() > 4
            || budgets
                .iter()
                .enumerate()
                .any(|(i, b)| budgets[..i].iter().any(|p| p.key == b.key))
        {
            return Err(LimitError::InvalidInput);
        }
        Ok(Self { budgets })
    }
    pub fn budgets(&self) -> &[Budget] {
        &self.budgets
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counter {
    pub rule: BudgetRule,
    pub used: u32,
    pub started_ms: u64,
    pub last_ms: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    Charge(Vec<Counter>),
    Limited { retry_after_ms: u32 },
}

/// The adapter must validate a durable enforcement generation and apply every
/// proposed counter atomically against the exact snapshot, or reject the attempt.
/// Missing counters are valid only when storage continuity is independently known.
pub fn plan(request: &Attempt, state: &[Option<Counter>], now_ms: u64) -> Result<Plan, LimitError> {
    if state.len() != request.budgets.len() {
        return Err(LimitError::InvalidInput);
    }
    let mut next = Vec::with_capacity(state.len());
    let mut retry_after_ms = 0;
    for (budget, counter) in request.budgets.iter().zip(state) {
        let current = current_counter(budget.rule, *counter, now_ms)?;
        if current.used == current.rule.limit {
            let remaining = current.started_ms + u64::from(current.rule.window_ms) - now_ms;
            retry_after_ms = retry_after_ms.max(remaining as u32);
        } else {
            next.push(Counter {
                used: current.used + 1,
                last_ms: now_ms,
                ..current
            });
        }
    }
    if retry_after_ms > 0 {
        Ok(Plan::Limited { retry_after_ms })
    } else {
        Ok(Plan::Charge(next))
    }
}

fn current_counter(
    rule: BudgetRule,
    state: Option<Counter>,
    now_ms: u64,
) -> Result<Counter, LimitError> {
    if let Some(counter) = state {
        let end = counter
            .started_ms
            .checked_add(u64::from(counter.rule.window_ms))
            .filter(|&end| end <= MAX_TIME)
            .ok_or(LimitError::UnsafeState)?;
        if counter.rule != rule
            || counter.used == 0
            || counter.used > rule.limit
            || counter.last_ms < counter.started_ms
            || counter.last_ms >= end
        {
            return Err(LimitError::UnsafeState);
        }
        if now_ms < counter.last_ms {
            return Err(LimitError::ClockRollback);
        }
        if now_ms < end {
            return Ok(counter);
        }
    }
    if now_ms
        .checked_add(u64::from(rule.window_ms))
        .is_none_or(|end| end > MAX_TIME)
    {
        return Err(LimitError::InvalidInput);
    }
    Ok(Counter {
        rule,
        used: 0,
        started_ms: now_ms,
        last_ms: now_ms,
    })
}

#[cfg(test)]
#[path = "../tests/unit/limiting.rs"]
mod tests;
