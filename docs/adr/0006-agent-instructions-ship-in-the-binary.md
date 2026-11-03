# The agent's instructions ship inside the binary

The prompt and JSON schema the agent runs with are compiled into each release and versioned with it; people configure only their Profile and never edit the instructions. The personal version is the opposite (the whole behaviour is one editable scheduled-task prompt), and editable prompts are what a reader would expect from an AI tool. We chose this so every install behaves the same and the rules stay testable: behaviour changes arrive by upgrading, and anything a person needs to vary belongs in the Profile.
