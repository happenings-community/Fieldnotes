# Requests & Offers v0.5.1 - Test Scenarios

Imported into Fieldnotes from the R&O test tracker. 57 scenarios across 12 sections.


## Installation & First Launch

### 1.1 - Go to the release page:
- https://github.com/happenings-community/requests-and-offers/releases/latest
- Mac users: choose Homebrew (recommended — see install guide) OR the DMG matching your chip (Apple Silicon = arm64, Intel = x64)
- Windows: download the .exe file
- Linux: choose AppImage (universal) or .deb (Debian/Ubuntu)

**Look for:**
- Download completes without errors.
- Note the file size and how long it took.

### 1.2 - Install the app for your platform:
- Mac DMG: open the .dmg, drag the app icon into your Applications folder
- Mac Homebrew: run 'brew tap happenings-community/requests-and-offers' then 'brew install --cask requests-and-offers'
- Windows: right-click the .exe → 'Run as administrator' → follow the wizard
- Linux AppImage: run 'chmod +x' on the file, then double-click to run

**Look for:**
- Installation completes without errors.
- Note any security warnings:
- Mac: Gatekeeper "unidentified developer" → fix via System Settings → Privacy & Security → Open Anyway
- Windows: SmartScreen warning → click "More info" → "Run anyway"

### 1.3 - Launch the app for the first time:
- Mac: open Finder → Applications → double-click "Requests and Offers" (or Cmd+Space, type the name)
- Windows: find "Requests and Offers" in your Start menu
- Linux: find it in your applications menu, or run from terminal

**Look for:**
- App opens without crashing.
- You should see a loading screen or landing page.
- Note how long it takes to open and anything unusual.

### 1.4 - Look for the connection status indicator in the navigation bar at the top of the app.
- Wait for it to change — this may take a moment.

**Look for:**
- Status changes to 'Connected'.
- Time how long this takes from launch (count seconds or use a stopwatch).
- Note if it stays on 'Connecting' for a long time or never connects.

## Profile Creation

### 2.1 - From the landing page or main menu, find and click the 'Create Profile' button or link.
- This should be visible on first use since you don't have a profile yet.

**Look for:**
- Profile creation form loads with fields to fill in.
- Note if the form appears quickly or if there's a delay.

### 2.2 - In the profile creation form, find the username field.
- Type in a username of your choice (e.g. your first name or a nickname).

**Look for:**
- Username field accepts your input.
- Note any character restrictions, length limits, or validation messages.

### 2.3 - Now we'll test that the bio field handles formatting properly.
- Copy the sample below into the bio field, then change the placeholder text (in square brackets) to your own details.
- Don't worry about what the asterisks, hashes and brackets do — that's what we're testing!
- SAMPLE (copy this):
- I'm **[your name or what you do]** based in *[your location]*.
- ### A bit about me
- - I'm interested in [something you care about]
- - I can help with [something you offer]
- - You can reach me via [how to contact you]
- Find out more at [Holochain](https://holochain.org)

**Look for:**
- The field accepts everything you pasted, including the asterisks, hashes and brackets.
- The formatting won't look pretty yet — it becomes properly styled text after you save in step 2.5.
- Note any special characters that get rejected or stripped.

### 2.4 - Fill in any other optional fields shown on the profile form.
- These might include: location, contact info, skills, or other details.
- You can use short placeholder text — e.g. 'test location', 'test@example.com'.

**Look for:**
- All optional fields accept input without errors.
- Note which fields are available and whether any have dropdowns, checkboxes, or other special inputs.

### 2.5 - Click the Save button to save your new profile.

**Look for:**
- Profile saves successfully.
- You should see a confirmation or be taken to your profile view.
- Your bio should now look properly formatted:
- **bold** appears bold
- *italic* appears italic
- ### A bit about me becomes a larger heading
- The bulleted list appears as bullets
- [Holochain](https://holochain.org) becomes a clickable link
- Note anything that doesn't render as expected.

## Profile Editing

### 3.1 - Navigate to 'My Profile' — look in the main menu or navigation bar for:
- A profile link
- Your name
- A user icon

**Look for:**
- Your existing profile loads with all the data from Test 2.
- Everything should match what you saved.

### 3.2 - Click 'Edit' to enter edit mode.
- You'll add a bit more to your bio to test that editing works.
- Paste this at the bottom of your existing bio, then change the placeholder text in square brackets:
- ### Recent updates
- 1. [Something you did recently]
- 2. [Something you're working on now]

**Look for:**
- The edit form loads with your existing bio already filled in.
- You can add to it without losing what's already there.

### 3.3 - Change another field besides bio.
- Update your location, contact info, or any other editable field to something different.

**Look for:**
- The field is editable and accepts your new input.
- Note which fields can and cannot be edited.

### 3.4 - Save your changes by clicking the Save/Update button.

**Look for:**
- Changes save successfully.
- Your new heading appears larger, and your numbered list displays properly (1. and 2. as numbered items).
- Your original bio content is still there and still formatted correctly.
- Any other field changes are reflected too.

## Creating a Request

### 4.1 - Navigate to the Requests section using the main menu.
- Look for a 'Create' or 'New Request' button.
- Click it.

**Look for:**
- Request creation form loads.
- Should have fields for title, description, and possibly service type.

### 4.2 - In the title field, paste this sample title:
- Test request — looking for a web designer
- (Feel free to change it to something else if you prefer.)

**Look for:**
- Title field accepts your input.
- Note any character limits.

### 4.3 - Now fill in the description. Copy the sample below into the description field.
- You can leave it as-is or change the placeholder text in square brackets — it doesn't matter for this test.
- SAMPLE (copy this):
- Looking for someone who can help with [a small project or task].
- ### What I need
- - [First thing you need]
- - [Second thing you need]
- Happy to discuss timing and details.

**Look for:**
- Description field accepts your input.
- Note if there's a rich text editor or just a plain text box.

### 4.4 - Look for a service type selector or category dropdown.
- If present, select an appropriate option.
- If not available, note this in your observations.

**Look for:**
- Service type selector works — options display and are selectable.
- OR note "No service types available" if missing/empty.

### 4.5 - Click Save to create your request.

**Look for:**
- Request saves successfully.
- Your title and description appear as you wrote them.
- You should see a confirmation or be taken to the detail view.

### 4.6 - Check your request appears in TWO places:
- 1. Navigate to the main Requests list (menu → Requests)
- 2. Check My Listings (if available)

**Look for:**
- Your request is visible in BOTH the Requests list AND My Listings.
- Title and description match what you entered.

## Creating an Offer

### 5.1 - Navigate to the Offers section using the main menu.
- Look for a 'Create' or 'New Offer' button.
- Click it.

**Look for:**
- Offer creation form loads.
- Should have similar fields to the request form.

### 5.2 - In the title field, paste this sample title:
- Test offer — offering facilitation services
- (Feel free to change it to something else if you prefer.)

**Look for:**
- Title field accepts your input.

### 5.3 - Now fill in the description. Copy the sample below into the description field.
- You can leave it as-is or change the placeholder text in square brackets — it doesn't matter for this test.
- SAMPLE (copy this):
- Available to help with [the kind of work you'd offer].
- ### What I can bring
- - [First skill or thing you'd offer]
- - [Second skill or thing you'd offer]
- Get in touch to chat about it.

**Look for:**
- Description field accepts your input.

### 5.4 - Select a service type if the option is available.
- If not, note this in observations.

**Look for:**
- Service type selector works.
- Or note if unavailable.

### 5.5 - Click Save to create your offer.

**Look for:**
- Offer saves successfully.
- Your title and description appear as you wrote them.

### 5.6 - Check your offer appears in TWO places:
- 1. Main Offers list (menu → Offers)
- 2. My Listings

**Look for:**
- Offer visible in BOTH the Offers list and My Listings.

## Archive & Reactivate Listings

### 6.1 - Go to My Listings.
- Find one of the requests or offers you created in Tests 4-5.

**Look for:**
- Your listings are displayed with management options.
- Note what actions are available (edit, archive, delete, etc).

### 6.2 - Click Archive (or equivalent) on one of your listings.
- Confirm if prompted.

**Look for:**
- Listing is archived.
- It should disappear from active listings or show 'Archived' status.
- Note what happens visually.

### 6.3 - Try to edit the archived listing.
- Click edit or try to modify it.

**Look for:**
- Note what happens:
- Can you edit archived listings?
- Does the app prevent it?
- Is there an error message?
- Any behaviour here is useful to document.

### 6.4 - Find the archived listing and reactivate it.
- Look for a Reactivate, Restore, or Unarchive button.

**Look for:**
- Listing returns to active status.
- It should reappear in the main listings.
- Confirm the content is unchanged.

## Search Functionality

### 7.1 - Go to the Requests page.
- Find the search box.
- Type a keyword from one of your request titles (from Test 4).
- For example, if you used the sample title, try searching 'designer'.

**Look for:**
- Search returns results containing your keyword.
- Note how fast results appear.
- Note whether search matches title, description, or both.

### 7.2 - Go to the Offers page.
- Search for a keyword from one of your offer titles (from Test 5).
- For example, if you used the sample title, try searching 'facilitation'.

**Look for:**
- Search returns your offer in the results.

### 7.3 - Go to All Users.
- Search for your own username or another known username.

**Look for:**
- Search finds the user profile.
- Note whether it searches username, display name, or other fields.

### 7.4 - Go to Organizations.
- Search for any organization name (if any exist).
- If none exist, just try using the search box.

**Look for:**
- If organizations exist, search finds them.
- If none exist, note whether the search box is present and works without errors.

### 7.5 - Search for a nonsense string that won't match anything.
- Example: 'xyzzy99999'

**Look for:**
- Search completes without errors.
- Shows empty results or a 'no results found' message.
- Note the exact message shown.

## Peer Discovery (KEY TEST)

### 8.1 - ⚠️ This test requires TWO people online at the same time.
- Coordinate with your testing partner:
- 1. Agree a specific time to both be online
- 2. Both launch the app
- 3. Both wait for 'Connected' status
- Do NOT proceed until both of you are connected.

**Look for:**
- Both testers have the app running and showing 'Connected'.
- Note the time you both came online.

### 8.2 - Confirm both testers have completed Tests 2-5:
- ✓ Both have a profile created
- ✓ Both have at least one request
- ✓ Both have at least one offer
- If either is missing something, create it now before continuing.

**Look for:**
- Both testers confirm they have profiles, requests, and offers.
- These must exist before testing discovery.

### 8.3 - Both testers: double-check you each have at least one request AND one offer visible in your own app.
- If either tester is missing a listing, create one now.

**Look for:**
- Both testers confirm their listings are visible in their own app.

### 8.4 - Tester A: Navigate to 'All Users'.
- Look through the list for Tester B's profile/username.
- If not visible, wait 2-3 minutes and refresh.
- Keep checking — peer discovery can take time.

**Look for:**
- Can Tester A see Tester B's profile?
- Record YES or NO.
- If yes, note how long it took to appear.

### 8.5 - Tester A: Navigate to Requests.
- Look for Tester B's request(s) in the list.

**Look for:**
- Can Tester A see Tester B's request?
- Note timing.

### 8.6 - Tester A: Navigate to Offers.
- Look for Tester B's offer(s) in the list.

**Look for:**
- Can Tester A see Tester B's offer?
- Note timing.

### 8.7 - Tester B: Now repeat the same checks:
- Go to All Users — can you see Tester A?
- Go to Requests — can you see Tester A's request?
- Go to Offers — can you see Tester A's offer?

**Look for:**
- Can Tester B see Tester A's data?
- IMPORTANT: Note if discovery is symmetric (both see each other) or asymmetric (one sees the other but not vice versa).

### 8.8 - While both still online:
- One tester creates a BRAND NEW listing (request or offer).
- The other tester watches their list to see if it appears.
- Don't refresh — just watch.

**Look for:**
- Does the new listing appear for the other tester?
- How long does it take?
- Does it need a page refresh or appear automatically?
- This tests real-time sync.

## Organizations

### 9.1 - Look in the main menu for an Organizations section.
- Click to create a new organization.

**Look for:**
- Organization creation form loads.
- Note what fields are available.

### 9.2 - Fill in the organization details.
- Use short placeholder text for name and other fields (e.g. 'Test Org').
- Look specifically for the 'Contact Person' field.
- Fill in all available fields.

**Look for:**
- All fields accept input.
- Contact Person field is present and functional.
- Note if it's a text field, dropdown, or links to user profiles.

### 9.3 - Now fill in the organization description. Copy the sample below.
- You can leave it as-is or change the placeholder text in square brackets — it doesn't matter for this test.
- SAMPLE (copy this):
- [Test Org] is a [short description of what the org does].
- ### What we do
- - [First activity or focus area]
- - [Second activity or focus area]
- Find us at [example link](https://example.org)

**Look for:**
- Description field accepts your input.

### 9.4 - Save the organization and view it.

**Look for:**
- Organization saves and displays correctly.
- Your description appears as you wrote it.
- Contact Person info is shown.
- All data matches what you entered.

### 9.5 - Navigate to the Organizations list page.
- Find your newly created organization.

**Look for:**
- Your organization appears in the list with correct name and details.

## Service Types

### 10.1 - Navigate to Service Types from the main menu.
- This section shows categories available for tagging requests and offers.

**Look for:**
- Service Types page loads and displays available categories.
- Note how many there are and what they're called.

### 10.2 - Browse through the available categories.
- Click on different types to see if they expand or show details.

**Look for:**
- Categories are browsable and interactive.
- Note the user experience — is it clear how these relate to listings?

### 10.3 - If you selected service types in Tests 4-5:
- Navigate to those listings.
- Check the categorisation is displayed correctly.

**Look for:**
- Listings show their assigned service type.
- Categorisation makes sense and is visible when browsing.

## Navigation & UI

### 11.1 - Systematically click through ALL main menu items:
- Requests
- Offers
- All Users
- Organizations
- Service Types
- My Profile
- My Listings
- Any others you can find

**Look for:**
- All menu items work and load their pages without errors.
- Note any broken links, pages that fail to load, or unexpected behaviour.

### 11.2 - Resize the app window to narrow width:
- Drag the window edge to make it narrower.
- Observe how the layout adapts.

**Look for:**
- Layout adjusts responsively — content should reflow, not overflow or overlap.
- Note any elements that break, overlap, or become unusable.

### 11.3 - At narrow width, look for a hamburger menu (☰ icon).
- Click/tap it to open.

**Look for:**
- Hamburger menu appears and opens when clicked.
- All navigation options are accessible.
- Note if any items are missing compared to the full-width menu.

### 11.4 - Browse the entire app and pay attention to text contrast:
- Can you read all text easily?
- Check: labels, placeholder text, disabled elements, coloured backgrounds.

**Look for:**
- All text is readable with sufficient contrast.
- Note any text that is hard to read, too light, or blends into its background.
- Accessibility matters!

## Network Resilience

### 12.1 - While the app shows 'Connected':
- Turn off your internet — toggle WiFi off or unplug ethernet.
- Watch what the app does.

**Look for:**
- The app should detect disconnection.
- Note: does the status change? How quickly?
- Is there a visual indicator?

### 12.2 - With internet still OFF, try to:
- Browse existing listings
- Create a new listing
- View profiles
- Navigate between pages

**Look for:**
- Note what works offline and what doesn't.
- Does the app show error messages? Does it crash?
- Can you still see previously loaded data?
- Record everything you observe.

### 12.3 - Turn your internet back on.
- Watch the app's connection status.

**Look for:**
- App reconnects and shows 'Connected'.
- Note how long reconnection takes.
- Check: was any data you tried to create offline saved or lost?
