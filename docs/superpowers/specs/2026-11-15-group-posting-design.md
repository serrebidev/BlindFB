# Group posting

Add Groups to the new-post Audience dialog. The user confirmed one group per
post. Choosing Groups opens a native, keyboard-accessible list of joined groups,
with type-ahead, loading/error status, retry, and cancellation. Fetch the joined
groups independently of the main view's cache and its 100-group read-ahead cap.
The picker must allow loading further groups rather than silently imposing a cap.
Keep a named browser page for the picker and resume its accumulated group scan
when Load more is pressed, so bounded reads advance instead of restarting at the top.
Close that page when the picker closes, including cancellation during a read.
Selection uses the group's canonical URL, never its display name or a temporary ref.

Reuse the existing text/photo/video posting flow and receipt verification. Extend
the backend with `post --group <group URL>`, mutually exclusive with `--to`.
Validate the destination before any browser mutation and open the selected group's
composer. Never fall back to posting on the profile. Do not publish a live test
post without separate authorization. Report pending moderation and uncertain
publication with advice to check the selected group before retrying.

## Approaches

The selected implementation approach is the shared backend composer extension:
it preserves validation, attachment support, and verified receipts. An external
browser would not provide the requested in-app flow. App-specific posting scripts
would duplicate the backend's existing composer and publishing safeguards.

## Scope and constraints

Keep Public, Friends, and Only me unchanged. No cross-posting, group management,
anonymous posting, or new dependencies. Use native controls, explicit accessible
names, Enter to choose, Escape to cancel, and logical focus. Translate new strings
into French, Russian, and Estonian. Release the backend before pinning it in the
app, then release signed app packages on all existing platforms.

Implementation details, error wording, and pagination follow repository patterns.

## Added scope: create a group

The user additionally requested creating new groups inside the app. Add File >
New group with a native form for the group name and Facebook's supported privacy
choices. Default to Private and show that choice explicitly. Include the supported
visibility choice for private groups if the live creation form requires/offers it.
The final button says Create group; Cancel and Escape do not create anything.
The backend receives explicit settings and verifies them before submitting.
Only a verified new group URL counts as success. Uncertain creation must tell the
user to check their groups before retrying, to avoid duplicate groups.

### Creation approaches

Use a native form backed by Facebook's own creation page. This keeps the entire
workflow in BlindFB and allows keyboard and screen-reader access. An external
browser link would not provide the requested in-app creation. A separate API
client would duplicate authentication and require unsupported access.

### Creation limits

Adding admins, cover photos, and other post-creation customization remain outside
this change unless Facebook requires them to create the group.
Do not create a real test group or invite anyone. Use fixtures, dry runs, verified
form state, and native control inspection. Release the new backend after .69,
then pin the app to the release that includes both features.

## Added scope: invite people

The user also requested inviting people to groups. Add Invite people to the native
group window, available for existing groups and newly created groups. Read the
people Facebook offers for that particular group; respect its permissions and
eligibility. Provide native selection, search/loading when the list needs them,
and an explicit Send invitations action that identifies the selected recipients.
Duplicate names must remain distinct through stable backend identifiers.

Cancel, Escape, and closing a dialog send no invitations. Selection alone never
sends. Confirm outcomes through Facebook and report failures or uncertain/partial
results honestly, without automatic retries that could duplicate invitations.
Do not send real invitations during development or testing. Creating a group does
not automatically invite anyone; invitations are a separate explicit action.

Reuse the native group window and Facebook's invitation picker through a named
backend page. Opening the website externally does not meet the in-app requirement;
inviting by guessed names or unsupported API calls is excluded.
