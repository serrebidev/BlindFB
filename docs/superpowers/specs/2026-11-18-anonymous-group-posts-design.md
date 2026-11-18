# Anonymous group posts and comments

## Intent

A tester can post and comment anonymously in Facebook groups on the phone and
expects the same in BlindFB.

## Findings (live, 2026-11-18)

- **Posts:** Facebook's website offers anonymous posting. 44 of 59 groups
  scanned on the test account expose
  `if_viewer_can_see_anonymous_post_toggle_button`. The composer has one
  `role=switch`. The first use shows a notice inside the composer, and turning
  it on changes the text box's label.
- **Comments:** the website shows the identity switcher only when
  `if_viewer_can_comment_anonymously`,
  `associated_group.if_viewer_can_use_nickname_on_comet` and an eligible second
  actor are all present. In the affected groups the server returns no nickname
  permission and `has_multiple_eligible_actors: false`. Facebook's mobile website
  redirects to the same page and offers nothing more.

## Approaches

1. **Drive Facebook's own composer switch (chosen).** It uses the control
   Facebook offers, works in any language and keeps the backend's existing checks.
2. **Send the anonymous actor directly to Facebook's private API.** Rejected: for
   comments it would bypass what the website withholds from the account, and it
   could not be tested without publishing.
3. **Automate the phone app.** Rejected: BlindFB is a desktop app with no phone
   session to drive.

## Design

- Backend: `post <text> --group URL --anonymous`.
- The switch is turned on before any text is typed, while Post is disabled.
- The notice is accepted only through an action that appeared after the switch
  was pressed, so Post can never be mistaken for it.
- `composeCheck` refuses to publish unless the switch is still on and the prompt
  still shows the anonymous author. There is no fallback to a named post.
- Receipts accept the anonymous actor. A post held for admins reports `pending`,
  so nobody is told to retry.
- App: after choosing a group, a dialog offers "With my name" or "Anonymously" and
  says that admins, moderators and Facebook can still see the author.
- Comments keep the existing identity switcher and the existing explanation.

## Non-goals

Anonymous comments where Facebook's website withholds the choice.

## Verification limits

Dry runs covered the switch, notice and draft in English, Estonian and Russian.
No real anonymous post was published during development.
