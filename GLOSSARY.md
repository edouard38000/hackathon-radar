# hackathon-radar

A self-hosted tool that finds hackathons a person can realistically attend, tracks their application deadlines, and texts them when something new or travel-funded appears.

## The person and their rules

**Profile**:
One person's settings that decide which hackathons are for them: Home, Countries, Train limit, Flight budget, the Travel-funded rule, Themes, Eligibility and Alert channel.
_Avoid_: Preferences, settings, config, account

**Home**:
The city a person travels from, with the airports they fly out of.
_Avoid_: Location, origin, base

**Countries**:
The countries a person will travel to at their own cost, as long as the trip also passes the Train limit or Flight budget.
_Avoid_: Allowlist, regions, scope

**Train limit**:
The longest train journey from Home, in hours, that still counts as going by train; anything longer counts as a flight.
_Avoid_: 4-hour rule, max train time

**Flight budget**:
The most a person will pay for a return flight from Home to attend a hackathon.
_Avoid_: Fare cap, price limit, €200 rule

**Travel-funded rule**:
Whether a Travel-funded hackathon counts anywhere in the world, regardless of Countries, Train limit and Flight budget.
_Avoid_: Défraiement rule, worldwide rule

**Theme**:
A subject a person wants hackathons about, such as AI agents or climate.
_Avoid_: Topic, track, category

**Eligibility**:
The facts about a person that organisers restrict on, such as student status, nationality or age.
_Avoid_: Requirements, restrictions

**Alert channel**:
Where a person receives Alerts.
_Avoid_: Notifier, messaging, notification method

## Hackathons and fit

**Hackathon**:
An in-person or hybrid event where participants build software, models, data products or hardware during the event.
_Avoid_: Event, competition, challenge, meetup

**Travel-funded**:
A Hackathon whose organiser covers part or all of a participant's travel or lodging.
_Avoid_: Défraiement, travel covered, sponsored

**Fit**:
A Hackathon fits a Profile when it passes all of the Profile's rules.
_Avoid_: Match, relevant, in scope

**Alert**:
A text message telling a person about Hackathons that newly fit them or newly became Travel-funded.
_Avoid_: Notification, ping, digest
