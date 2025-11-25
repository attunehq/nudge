## Claude-QA (better name TBD)

Problem statement:
- Claude is not good at following instructions in claude.md or even the prompt, especially as context window fills.
- As a heavy claude code user, this is my primary pain point with claude.
  - My claude.md is littered with guidance, so much so that i have other guidance files linked from it because it was getting too large
  - Despite this claude _routinely_ has a hard time following instructions, especially as tasks get longer
- A common refrain among AI evangelists is "don't care what the underlying code is doing, treat it like a compiler" but:
  - The specific implementation often matters
  - Claude is not yet smart enough to be trusted to implement the code following instructions

How we'll address this:
- Our daemon is an attempt to help with this by relying on engineering expertise instead of hoping the model magically does it.
- We accomplish this by building rules:
  - When possible, these are deterministic rules like regex matches or similar
  - Otherwise, these are natural language rules evaluated by a smaller and quicker model
  - The theory here is that when models are run with fresh context and a good system prompt, they are good at evaluating "does this do X"
  - From there, we use the same small model to build a tailored instruction for claude, something like "do Y instead" but tailored to the specific context
- In effect, we're trying to build an omni "memory" for claude by adding "teachability" to the model, layered on top of the existing claude code loop so that we can offer it as an addon.

What is it:
- Hooks claude code (and in the future other agents) (and in the future maybe our own custom agent with this embedded)
- Watches actions
- Records times when users have to intervene
  - Example 1:
    - Claude tries to run a command that fails, like "compile and run the server" but the server needs the database to run
    - The user interrupts it as claude goes down a rabbit hole and says "i fixed it"
    - Our daemon notices this and injects a prompt to claude to ask the user what they did for next time
    - When the daemon notices claude do something similar in the future, it injects guidance based on what the user did last time
  - Example 2:
    - Claude writes some code
    - The user says "instead of X, do Y"
    - Our daemon notices this and records that guidance for next time
    - In the future, when the daemon sees claude do something similar, it interrupts claude and injects similar guidance as before
  - Example 3:
    - User explicitly adds a rule/guidance to our tool's configuration
    - When the daemon notices claude violating the rule/guidance, it interrupts claude and injects guidance based on the user's input
  - Example 4:
    - User has configured rules in claude.md or other files in the repository
    - When we initially set up the daemon, it looks through the configured rules (we need an initial setup step for hooks anyway)
    - It then interacts with the rules as described in other examples

Open questions to answer through experimentation:
- Is the theory that smaller, faster models are able to judge and steer the mail claude code loop correct?
- What is the rate of errors that we can reduce?
- Is it possible to build a product out of this?

Milestones:
1. Intial prototype with static rules only
  - Our daemon interacts with 4.5 haiku using the local `claude` cli tool in non-interactive mode
  - Our daemon integrates with the main claude code via claude hooks functionality: https://code.claude.com/docs/en/hooks-guide
  - This version has static rules pre-programmed, it does not learn from user input or the claude.md file
2. Working prototype with dynamic rules
  - This milestone extends the previous one by adding the ability to learn from user input and the claude.md file
3. Systematically evaluate the prototype if it seems promising subjectively
  - We'll need to come up with a good test harness and metrics to evaluate
  - This means we'll need to build a way to programmatically interact with the main claude code interface as well, to be able to set up a test harness

Deliverables and timelines (we'll ripcord if at any point it seems like we're wasting our time):
1. Initial static-only prototype: less than one day
2. Working dynamic prototype: initial budget 3 days
  - Daemon or stateful CLI that responds to hooks and injects guidance using deterministic rules and haiku with natural language
  - Successfully demonstrate "here is a problem that exists, here is me providing guidance, here is claude following the guidance"
3. Evaluate the prototype: initial budget 1 week
  - Build test harness to evaluate the prototype
  - Run evaluations
  - Plot results: something something "given this test case, the number of user interventions drops from X to Y"
4. Write up a post for Hacker News, goal is to make the front page: 1-2 days
5. From there evaluate whether we should productize this: unknown timeline
6. If so, build a product: unknown timeline

Future expansion:
- If we can make this work and it is effective, we'll consider building a product out of it:
  - Integrate into all local agents, including codex and gemini (if possible: they don't all have hooks)
  - Investigate integrating into web-hosted agents somehow (e.g. hosted codex or claude) although it's not yet known if this is possible
  - Build our own custom "claude code"-like agent interface, possibly that is model agnostic, with these features built in
  - Investigate using an even smaller model than Haiku, maybe one that runs locally
    - From interacting with the claude code subreddit, people are already very frustrated about claude code usage limits, so us using the same claude code plan (and its limits) is likely not a good idea longer term if we productize
- We may be able to extend this to a more generic "does this do what the user wants" system
  - Effectively add a two step "implement, then review" to claude code's loop
  - When the user provides a prompt, while claude is implementing it our daemon builds a set of test evaluations by which to judge the output
  - When claude says it's finished implementing, our daemon then evaluates it for how well it satisfies the the user's intent

Non-goals:
- At the moment, we're not trying to optimize latency: we'll optimize for correctness instead. Over time we can make it faster(?)
- At the moment, we're not trying to optimize for cost: we'll optimize for correctness instead. Over time we can make it cheaper(?)
