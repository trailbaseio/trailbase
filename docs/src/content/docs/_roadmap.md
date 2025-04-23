Over time, we would like to make TrailBase the best application base it can be.
Tell us what's missing, what could be better, and what smells.
Independently, we're very open to contributions, just talk to us first so we
can figure out how any feature will fit into the overall picture and minimize
friction.
For context, some larger features we have on our Roadmap:

- Auth: more customizable settings, more customizable UI, and multi-factor.
  Also, service-accounts to auth other backends as opposed to end-users.
- Many SQLite databases: imagine a separate database by tenant or user. Note
  that while joins are available across databases, foreign keys are not.
- A message queue system to deal with slow tasks or bursty workloads.
- We might want to address fan-out and the integration of external resources
  through GraphQL or similar.
