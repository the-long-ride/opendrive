//! Static step content for the Google Cloud Console setup guide modal.

pub(super) type Seg = (&'static str, bool);
pub(super) type StepItem = (&'static str, &'static [&'static [Seg]]);

pub(super) const OAUTH_STEPS: &[StepItem] = &[
    (
        "Choose Personal or External OAuth",
        &[
            &[
                ("Open ", false),
                ("console.cloud.google.com", true),
                (" and sign in.", false),
            ],
            &[
                ("For one Google account, find ", false),
                ("Quick setup for personal use (Personal mode)", true),
                (
                    " in Google Cloud Help and create a Desktop app client.",
                    false,
                ),
            ],
            &[
                ("Enable Drive API in Step 2, then go to Step 6.", false),
                (
                    " Personal mode skips the unverified-app warning and seven-day testing expiry.",
                    false,
                ),
            ],
            &[
                ("Personal mode still requires a ", false),
                ("Client ID and Client Secret", true),
                (
                    ". Sign in as the Google account that created the app.",
                    false,
                ),
            ],
            &[
                (
                    "For multiple accounts or an existing External project, use ",
                    false,
                ),
                ("External", true),
                (
                    " and follow all steps. To create a project, click the ",
                    false,
                ),
                ("project picker", true),
                (" (top-left) -> ", false),
                ("New Project", true),
                (" -> name it (e.g. ", false),
                ("OpenDrive", true),
                (") -> ", false),
                ("Create", true),
                (".", false),
            ],
        ],
    ),
    (
        "Enable the Google Drive API",
        &[
            &[
                ("Open ", false),
                ("APIs & Services -> Library", true),
                (".", false),
            ],
            &[
                ("Search ", false),
                ("\"Google Drive API\"", true),
                (", open it, and click ", false),
                ("Enable", true),
                (".", false),
            ],
        ],
    ),
    (
        "External: Configure Google Auth platform",
        &[
            &[
                ("Open ", false),
                ("Google Auth platform", true),
                (" (or ", false),
                ("OAuth consent screen", true),
                (") and click ", false),
                ("Get Started", true),
                (".", false),
            ],
            &[
                ("Set App name (", false),
                ("OpenDrive", true),
                ("), Support email, ", false),
                ("External", true),
                (" audience, and Developer email.", false),
            ],
        ],
    ),
    (
        "External: Audience & Drive Scope",
        &[
            &[
                ("Under ", false),
                ("Audience", true),
                (", add your Gmail to ", false),
                ("Test users", true),
                (" (7-day token expiry) OR click ", false),
                ("Publish app -> Confirm", true),
                (".", false),
            ],
            &[
                ("Under ", false),
                ("Data Access -> Add or remove scopes", true),
                (", add ", false),
                ("https://www.googleapis.com/auth/drive", true),
                (" (or ", false),
                (".../drive.readonly", true),
                (") and ", false),
                ("Save", true),
                (".", false),
            ],
        ],
    ),
    (
        "External: Create a Desktop App OAuth Client",
        &[
            &[
                ("Open ", false),
                ("Clients -> + Create Client", true),
                (" (or ", false),
                (
                    "Credentials -> + Create Credentials -> OAuth client ID",
                    true,
                ),
                (").", false),
            ],
            &[
                ("Choose Application type: ", false),
                ("Desktop app", true),
                (" -> ", false),
                ("Create", true),
                (" (loopback ", false),
                ("http://127.0.0.1", true),
                (" works automatically).", false),
            ],
        ],
    ),
    (
        "Paste credentials & connect in OpenDrive",
        &[
            &[
                ("Copy ", false),
                ("Client ID", true),
                (" (", false),
                ("...apps.googleusercontent.com", true),
                (") & ", false),
                ("Client Secret", true),
                (" (", false),
                ("GOCSPX-...", true),
                (").", false),
            ],
            &[
                ("Click ", false),
                ("Sign in with Google", true),
                (" (or ", false),
                ("Copy Link", true),
                ("). If unverified warning appears, click ", false),
                ("Advanced -> Continue", true),
                (".", false),
            ],
        ],
    ),
];

pub(super) const SERVICE_ACCOUNT_STEPS: &[StepItem] = &[
    (
        "Enable Google Drive API",
        &[&[
            ("Enable the ", false),
            ("Google Drive API", true),
            (" in your project as shown in Option A Step 2. No OAuth client is needed.", false),
        ]],
    ),
    (
        "Create a Service Account & JSON Key",
        &[
            &[
                ("Go to ", false),
                (
                    "IAM & Admin -> Service Accounts -> + Create Service Account",
                    true,
                ),
                (" -> ", false),
                ("Done", true),
                (".", false),
            ],
            &[
                ("Open it -> ", false),
                ("Keys -> Add key -> Create new key -> JSON -> Create", true),
                (" to download the ", false),
                (".json", true),
                (" file.", false),
            ],
        ],
    ),
    (
        "Share Drive folders with client_email",
        &[
            &[
                ("Copy ", false),
                ("client_email", true),
                (" (", false),
                ("...@<project>.iam.gserviceaccount.com", true),
                (") from the JSON.", false),
            ],
            &[
                ("In a shared ", false),
                ("My Drive", true),
                (
                    " folder, Editor can modify existing files but cannot upload new files as the service account.",
                    false,
                ),
            ],
            &[
                ("For new uploads, grant access to add files in a ", false),
                ("Shared Drive", true),
                (" or use browser sign-in to upload to My Drive as a Google user.", false),
            ],
        ],
    ),
    (
        "Connect in OpenDrive",
        &[&[
            ("Select ", false),
            ("Service Account JSON (No Login)", true),
            (", drop/paste the JSON, and click ", false),
            ("Connect Service Account", true),
            (".", false),
        ]],
    ),
];

pub(super) const TROUBLESHOOTING_TIPS: &[StepItem] = &[
    (
        "Missing scope / insufficient permissions (403)",
        &[&[
            ("Open ", false),
            (
                "Google Auth platform -> Data Access -> Add or remove scopes",
                true,
            ),
            (", check ", false),
            ("https://www.googleapis.com/auth/drive", true),
            (" (or ", false),
            (".../drive.readonly", true),
            ("), ", false),
            ("Save", true),
            (", then re-auth.", false),
        ]],
    ),
    (
        "403 access_denied (app in Testing)",
        &[&[
            ("Add your Gmail under ", false),
            ("Audience -> Test users", true),
            (", or click ", false),
            ("Publish app", true),
            (".", false),
        ]],
    ),
    (
        "redirect_uri_mismatch",
        &[&[
            ("Ensure OAuth client Application type is ", false),
            ("Desktop app", true),
            (", not Web application.", false),
        ]],
    ),
    (
        "Multiple Google accounts",
        &[&[
            ("Use ", false),
            ("External", true),
            (
                " for multiple accounts. Personal mode only works for the account that created the app.",
                false,
            ),
        ]],
    ),
];
