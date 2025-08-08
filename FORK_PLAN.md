# TrailBase Fork Implementation Plan

## Context
This is StuMason's fork of TrailBase (https://github.com/trailbaseio/trailbase). The goal is to implement and demonstrate fixes for critical UX/logging issues identified in issue #122 (https://github.com/trailbaseio/trailbase/issues/122).

## Background
While working with TrailBase, several critical issues were identified:
1. **Forms Problem**: Settings forms use confusing nullable checkboxes that behave like direct database entry forms rather than user-friendly web forms
2. **Toast Errors**: Error messages appear in toasts but are non-actionable (can't select/copy text, disappear quickly)
3. **Missing Error Logs**: User-facing errors (shown in toasts) are never captured in the database logs - they only go to stdout/stderr, making production debugging impossible without SSH access

## Maintainer Response
ignatz (main maintainer) was supportive but had concerns:
- Agreed forms need simplification
- Worried about sensitive data in logs
- Wants to separate request logs from debug logs
- Open to improvements with careful implementation

## Implementation Strategy
Create separate PRs in this fork to demonstrate each fix independently, then show to ignatz as working examples.

### PR 1: Fix Form UX (Priority: HIGH)
**Branch**: `fix-smtp-forms`
**Goal**: Make settings forms behave like normal web forms
**Changes**:
- Remove nullable checkboxes from SMTP settings
- Treat empty fields as "don't use" (intuitive behavior)
- Specifically fix the SMTP username/password fields that currently send `""` instead of null
**Files to modify**:
- Admin UI form components (likely in trailbase-assets/js/admin/src/)
- Focus on SMTP configuration section

### PR 2: Actionable Toast Errors (Priority: MEDIUM)
**Branch**: `fix-toast-errors`
**Goal**: Make error toasts useful for debugging
**Changes**:
- Replace current toast library with one supporting text selection (e.g., Sonner or react-hot-toast)
- Add "Copy Error" button to error toasts
- Increase display duration for error toasts
- Ensure error text is selectable
**Files to modify**:
- Toast notification components
- Error handling in admin UI

### PR 3: Capture User-Facing Errors (Priority: HIGH)
**Branch**: `add-error-logging`
**Goal**: Log errors that users actually see (not debug logs)
**Changes**:
- When API returns 4XX/5XX, capture the error message sent to client
- Add error_message column to logs display (or separate error log)
- Only log what's already sent to users (addresses security concerns)
- Make errors searchable in admin UI
**Files to modify**:
- `trailbase-core/src/admin/error.rs` - Capture errors during response
- `trailbase-assets/js/admin/src/components/logs/LogsPage.tsx` - Display errors
- Database schema for logs table (if adding columns)

## Key Principle
We're NOT trying to capture all debug/server logs. We're capturing **user-facing operational errors** - the exact error messages that users already see in toasts but currently disappear into the void.

## Success Criteria
1. Forms work intuitively without database-like nullable checkboxes
2. Error toasts can be copied/saved for debugging
3. Admin UI shows the same errors users see, making production debugging possible without SSH

## Next Steps
1. Create `fix-smtp-forms` branch
2. Implement the form fixes (smallest, least controversial change)
3. Test thoroughly
4. Show working example to ignatz
5. Based on feedback, proceed with other PRs

## Repository Setup
```bash
# Your fork
origin: https://github.com/stumason/trailbase.git

# Original repo (add as upstream)
upstream: https://github.com/trailbaseio/trailbase.git

# Keep fork updated
git fetch upstream
git rebase upstream/main
```

## Current Status
- Fork created
- On branch: `logging-filters` (can be cleaned up/removed)
- Ready to create first feature branch for form fixes