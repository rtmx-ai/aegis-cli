# GEMINI.md: Aegis CLI Architectural Blueprint

### Project: aegis-cli

### Repository: https://github.com/rtmx-ai/aegis-cli

### License: Apache 2.0

### Target Audience: Software Engineers within the Department of War and the Defense Industrial Base (DIB).

---

## Preface: Engineering SDLC & Quality Assurance

Building an agentic AI tool for Controlled Unclassified Information (CUI) environments requires a paradigm shift from standard open-source development. Every feature, boundary, and cloud interaction in `aegis-cli` must be provably secure, strictly traceable to a requirement, and deterministically testable.

To achieve the high-assurance standards required by the Department of Defense (DoD) and the Defense Industrial Base (DIB), the `aegis-cli` project strictly adheres to the following engineering principles governing its Software Development Lifecycle (SDLC).

### 0.1 Requirements & Specifications (The RTMX standard)

Code is never written in a vacuum. Every pull request must trace back to a defined requirement or specification.

* **Requirements Traceability Matrix (RTM):** All functional and compliance capabilities (e.g., HITL enforcement, VPC-SC routing) are tracked. `GEMINI.md` serves as the architectural foundation, but atomic requirements will be managed using the RTMX standard to ensure bi-directional traceability from NIST 800-171 controls down to the specific Jest/Vitest test case executing in CI.
* **Specification-First:** Before altering the Vertex AI payload schema, the local state machine, or the `ink` UI components, the specification must be drafted, reviewed, and approved.

Here is the significantly enriched **Testability & Quality Assurance** section. When building a tool that acts as a trust boundary for Controlled Unclassified Information (CUI), testing cannot be an afterthought; it must be the architectural foundation. We need to define exactly how the local development harness works and how the testing pyramid validates every Behavior-Driven Development (BDD) specification.

You can replace the previous Sections 0.2 and 0.3 in the Preface with this expanded, comprehensive version.

---

### 0.2 Local Development Environment & Test Harnesses

A strong, deterministic local development environment is critical to maintaining developer velocity without compromising the security posture of the host machine.

* **The Local Sandbox:** Developers will not run the development build of `aegis-cli` against their actual workstation's file system or real GCP projects by default. The local environment utilizes a strict sandbox.
* **Mocking the Brain (Vertex AI Harness):** To enable offline development and prevent rapid iteration from incurring cloud costs or hitting API rate limits, the test harness intercepts Vertex AI SDK calls. We will utilize a local mock server or dependency-injected stubs that return deterministic `functionCall` payloads (e.g., simulating the AI asking to read a file) and simulated Markdown streams.
* **Mocking the Edge (File System & Prompts):** Operations utilizing `fs` and `child_process` are routed through an abstraction layer. During testing, `memfs` (an in-memory file system) is injected to simulate local workspaces, and the `@clack/prompts` UI is intercepted to simulate a user pressing `Y`, `N`, or typing a modified command.

### 0.3 The Testing Pyramid: Unit, Integration, and E2E

To fully verify and validate our RTMX requirements and BDD feature specifications, `aegis-cli` implements a rigorous, three-tiered testing strategy. Every pull request must pass all three tiers in an isolated CI pipeline before merging.

#### Tier 1: Unit Testing & TDD (The Foundation)

* **Objective:** Verify the internal logic of individual functions, state machines, and context tokenization.
* **Implementation:** Utilizing `Vitest` (or `Jest`), unit tests are written *before* the implementation code (TDD).
* **Focus Areas:** * Validating the `.aegisignore` path matching logic.
* Ensuring the token calculator correctly estimates payload sizes.
* Verifying that the Reducer state machine correctly transitions from `EVALUATING` to `ACTION_REQUIRED`.



#### Tier 2: Integration Testing (The Seams)

* **Objective:** Verify the boundaries where `aegis-cli` interacts with external systems (Google Cloud API, Pulumi Automation API, and the local OS execution environment).
* **Implementation:** These tests operate with live, but sandboxed, dependencies.
* **Focus Areas:**
* **Cloud Infrastructure Seam:** Executing the embedded Pulumi TypeScript locally against a "dry-run" configuration to ensure the generated GCP resource graph is structurally valid and compliant.
* **Execution Seam:** Spawning actual, non-destructive shell commands via `child_process` (e.g., `echo "test"`) and ensuring the CLI correctly captures `stdout` and `stderr` without leaking memory.



#### Tier 3: Functional & End-to-End (E2E) Testing (The User Journey)

* **Objective:** Validate the BDD specifications by executing the fully compiled `aegis` binary from the outside, exactly as a defense engineer would use it.
* **Implementation:** Using an execution wrapper (like `execa` or a dedicated CLI testing framework), the E2E suite spins up a temporary directory, initializes a dummy Git repository, and executes the compiled `aegis-cli` binary.
* **Focus Areas:**
* **The HITL Workflow:** The E2E harness pipes `stdin` to the CLI, simulating a user typing a prompt that requires a file change. It waits for the visual `ink` prompt to mount, pipes a `Y` keystroke, and asserts that the file was actually modified in the temporary directory.
* **Audit Ledger Verification:** After the binary exits, the E2E suite parses the resulting `~/.aegis/logs/*.jsonl` file. It mathematically asserts that the session metadata, the exact command, and the simulated user's approval timestamp were irrefutably logged, fulfilling the NIST 800-171 auditing requirement.

### 0.4 Requirements Traceability (Verification vs. Validation)

Every test written in the tiers above must trace back to a specific RTMX requirement or BDD scenario.

* **Verification (Did we build the tool right?):** Addressed by Unit and Integration tests. (e.g., *Is the Vertex AI payload formatted according to Google's schema?*)
* **Validation (Did we build the right tool?):** Addressed by Functional/E2E tests. (e.g., *Can the defense engineer successfully authorize a secure code change while satisfying the CUI logging control?*)

By treating the test harness as a first-class citizen alongside the production code, we ensure that the "Body of Evidence" we provide to an auditor is backed by mathematical certainty, not just good intentions.

### 0.5 Behavior-Driven Development (BDD) for Compliance

While TDD verifies that the code works correctly, Behavior-Driven Development (BDD) verifies that the system behaves compliantly. We utilize BDD (via tools like Cucumber or strictly formatted Jest scenarios) to prove our security boundaries to auditors.

* **Given-When-Then Scenarios:** All trust boundaries and Human-in-the-Loop (HITL) interactions must be defined behaviorally.
> **Given** the AI agent proposes a state-mutating command (`npm install`)
> **When** the prompt is intercepted by the local execution boundary
> **Then** the UI must pause and display a Y/N authorization prompt
> **And** the system must not execute the command without a `Y` input
> **And** the action must be recorded in the local `.jsonl` audit ledger.


* **Executable Documentation:** These BDD scenarios serve as living, executable documentation that proves CUI controls are actively enforced in the latest build.

### 0.6 Deployability & Continuous Delivery (CD)

Delivering `aegis-cli` to defense networks requires strict adherence to secure supply chain principles (e.g., SLSA framework). The CI/CD pipeline is fully automated and designed for continuous delivery.

* **Reproducible Builds:** The `npm` package must be built deterministically. The same source code must always produce the exact same binary hash.
* **Software Bill of Materials (SBOM):** Every release automatically generates and publishes a CycloneDX or SPDX SBOM, detailing every transitive dependency. This is a non-negotiable requirement for DoD software ingestion.
* **Automated Security Scanning:** Continuous integration pipelines must run SAST (Static Application Security Testing), dependency vulnerability scanning (e.g., `npm audit`, Snyk), and secret-leakage detection on every commit.
* **Signed Releases:** All commits must be verified via GPG/SSH signatures. The final `npm` package artifacts and GitHub releases are cryptographically signed, ensuring DIB engineers can verify the integrity of the tool before installing it on a CUI-handling workstation.

---

## 1. Executive Summary & North Star

### 1.1 Problem Statement

Modern software engineering relies heavily on AI-assisted coding and agentic workflows to maintain velocity. However, engineers operating within the Department of Defense (DoD) and the broader Defense Industrial Base (DIB) are frequently cut off from these tools. Recent federal bans on commercial platforms (like Anthropic's Claude) and the stringent requirements of NIST SP 800-171 and CMMC level 2.0 mean that defense engineers are forced to choose between security compliance and developer productivity. Existing "enterprise" AI solutions often lack the terminal-native, agentic capabilities required for rapid local development, and deploying them securely within Impact Level 4 (IL4) or IL5 boundaries is a complex, bespoke infrastructure challenge.

### 1.2 The Solution: `aegis-cli`

The `aegis-cli` (Aegis) is an open-source (Apache 2.0) project designed to solve this exact problem. Delivered via `npm`, Aegis is a terminal-native, agentic AI pair programmer explicitly engineered for CUI environments. It bridges the gap between the local developer workstation and secure cloud boundaries by unifying two components into a single developer experience:

1. **The Edge Client (`aegis-cli`):** A Node.js/TypeScript reactive terminal application (built with `ink`) that manages local file context, enforces Human-in-the-Loop (HITL) execution constraints, and maintains immutable local audit ledgers.
2. **The Cloud Boundary (Aegis Backend):** A Google Cloud Assured Workloads environment, deterministically provisioned via embedded Pulumi TypeScript, ensuring data sovereignty, zero retention, and Customer-Managed Encryption Key (CMEK) protection for Vertex AI Gemini interactions.

### 1.3 North Star Principles

* **Frictionless UX, Uncompromising Security:** The developer experience must rival the best commercial tools (e.g., instantaneous startup, streaming responses, reactive UIs) without bypassing a single NIST 800-171 control.
* **Zero-Trust Context Management:** The agent only "sees" what it is explicitly authorized to see. Context is ephemeral (in-memory only), strictly filtered by `.aegisignore`, and never retained by the cloud provider.
* **Irrefutable Auditability:** Every action the agent takes—whether reading a file or executing a shell command—must be cryptographically or deterministically logged, enabling compliance officers to reconstruct the exact "body of evidence" for any session.
* **Decentralized-Ready:** The architecture must preserve "trade space" for future edge computing features, such as RTMX Sync, peer-to-peer CI monitoring, and localized LLM inference.

---
Here is the enriched **Section 2: Onboarding & Operational Modes**. This section details the initialization state machine, credential management, and cryptographic boundaries necessary to bootstrap trust on a local workstation.

You can append this directly to your `GEMINI.md` file below Section 1.

---

## 2. Onboarding & Operational Modes

Bootstrapping a secure agentic environment on a local workstation is the most critical phase of the `aegis-cli` lifecycle. If the initial connection to the cloud boundary is compromised or misconfigured, all subsequent CUI handling is invalid.

To support the diverse network topologies and IAM restrictions within the Defense Industrial Base (DIB), the `aegis init` command utilizes a reactive state machine to route users through one of three highly structured operational modes.

### 2.1 The `aegis init` State Machine

When a developer executes `aegis init`, the CLI launches an `@clack/prompts` driven interactive flow. This is not a static configuration script; it is a state machine that dynamically probes the local environment (checking for existing GCP credentials, OS keychain access, and network egress rules) before finalizing the configuration.

* **State 0: Environment Probe:** Scans for existing `gcloud` Application Default Credentials (ADC) and checks environment variables for proxy settings or CA bundles (critical for corporate TLS inspection).
* **State 1: Mode Selection:** Prompts the user to select their deployment topology: Self-Service, Enterprise, or Managed SaaS.
* **State 2: Credential Negotiation:** Executes the specific authentication flow required for the chosen mode.
* **State 3: Infrastructure Binding:** Connects to the backend infrastructure (or provisions it) and retrieves the required endpoints and KMS key references.
* **State 4: Configuration Commit:** Writes the immutable environment state to `~/.aegis/config.yaml` with strict POSIX file permissions (`600`).

### 2.2 Mode 1: Self-Service BYOC (Bring Your Own Cloud)

**Target Persona:** Independent defense contractors or members of agile "skunkworks" teams who possess Google Cloud Project Creator permissions but lack dedicated DevSecOps support.

* **The Flow:** The CLI detects active GCP credentials. Using the embedded `@pulumi/pulumi/automation` package, the Node.js process silently orchestrates a Pulumi stack update. It provisions a hardened, single-tenant Assured Workloads boundary (US regions only), configuring VPC Service Controls, Cloud KMS (CMEK), and Cloud Audit Logs.
* **Credential Management:** Relies entirely on short-lived Google Cloud ADC (`gcloud auth application-default login`). `aegis-cli` never touches, stores, or transmits long-lived service account keys.
* **Security Boundary:** The boundary is enforced at the GCP Project level. The CLI binds locally to the newly generated private Vertex AI endpoint, routing all traffic exclusively to that destination.

### 2.3 Mode 2: Enterprise BYOC

**Target Persona:** Engineers operating within mature, locked-down defense enterprise networks where infrastructure is centrally managed and developer workstations have strict outbound firewall rules.

* **The Flow:** The CLI prompts the user for their corporate Aegis endpoint URL (e.g., `https://aegis-gateway.internal.defense-corp.mil`). No local infrastructure provisioning occurs.
* **Credential Management:** Integrates with Workforce Identity Federation. The CLI may launch a local web server to capture an SSO callback or rely on an existing enterprise identity proxy (like Google Cloud Identity-Aware Proxy) configured on the workstation.
* **Security Boundary:** The boundary is enforced at the Google Cloud Organization level. VPC Service Controls (VPC-SC) ensures that the enterprise Vertex AI endpoint will *only* accept traffic originating from authorized corporate IP ranges and managed devices, mitigating the risk of stolen developer laptops.

### 2.4 Mode 3: Managed SaaS

**Target Persona:** Organizations or individuals who require CUI-compliant AI capabilities but do not wish to manage their own Google Cloud infrastructure or Pulumi deployments.

* **The Flow:** `aegis init` utilizes an OAuth 2.0 Authorization Code Flow with Proof Key for Code Exchange (PKCE). It opens the default system browser, routing the user to a centralized, FedRAMP-authorized identity broker (e.g., Auth0).
* **Credential Management:** Upon successful authentication, the broker returns a short-lived JSON Web Token (JWT). To comply with CMMC, `aegis-cli` *must not* store this token in plaintext. It interfaces directly with the host operating system's secure credential store (e.g., macOS Keychain, Windows Credential Manager, or Linux Secret Service API) via a native Node.js module (like `keytar` or an equivalent modern standard).
* **Security Boundary:** Strict tenant isolation within the Managed Aegis GCP environment. The backend infrastructure utilizes dedicated service accounts and isolated Vertex AI endpoints per tenant, ensuring that one defense contractor's data cannot inadvertently mix with another's.

### 2.5 The Configuration Artifact (`~/.aegis/config.yaml`)

The result of the `init` state machine is the local configuration file. To satisfy local endpoint security controls, this file is generated with strict least-privilege permissions (`chmod 600`), readable and writable only by the executing user.

It contains *no* secrets, tokens, or CUI. It acts purely as a routing table and policy definition for the agent:

```yaml
version: "1.0"
mode: "self-service" # or "enterprise", "managed"
auth:
  method: "gcp-adc" # "sso" or "os-keychain"
backend:
  projectId: "def-corp-aegis-prod-1"
  region: "us-central1"
  endpointUrl: "https://us-central1-aiplatform.googleapis.com/v1/..."
infrastructure:
  vpcName: "aegis-secure-vpc"
  kmsKeyId: "projects/.../cryptoKeys/aegis-gemini-key"
  auditLogBucket: "aegis-audit-logs-1a2b3c"

```

---

## 3. Data Model & Lifecycle Architecture

When operating within a DoD Impact Level 4/5 (IL4/IL5) equivalent boundary or handling Controlled Unclassified Information (CUI), the data model is dictated by the principle of least privilege and strict data containment. `aegis-cli` acts as a transient broker; it does not "store" CUI, it only processes it ephemerally.

The data architecture is strictly divided into two zones: **The Edge** (the developer's workstation operating in user-space RAM) and **The Boundary** (the Google Cloud Vertex AI endpoint).

### 3.1 Data Consumption & Containment (Inputs)

Aegis only ingests data when explicitly authorized by the developer or requested by the AI agent via a verified function call.

* **The Context Payload (CUI):** Source code, configuration files, and local logs are consumed strictly as UTF-8 plaintext.
* **The `.aegisignore` Filter:** Before the `read_file` tool executes, the requested path is evaluated against the `.aegisignore` file (which inherits from `.gitignore` by default but adds mandatory blocklists for `.env`, `*.pem`, `~/.aws/credentials`, etc.). If the agent attempts to read a blocked file, the CLI intercepts the call, denies access, and feeds a localized permission error back to the model.
* **Size & Token Limits:** To prevent accidental memory exhaustion or massive data egress, local files are subject to a strict size threshold (e.g., 1MB per file). Files exceeding this limit are truncated or rejected, ensuring the CLI process footprint remains small and responsive.

### 3.2 Data Production & Mutation (Outputs)

Aegis produces two types of data: transient visual output and persistent state mutations.

* **Transient Output (AI Responses):** The Markdown-formatted text streaming from Vertex AI is rendered dynamically in the terminal via the `ink` React UI. This data lives only within the terminal buffer and Node.js process memory.
* **Persistent Mutations (Agentic Actions):** When the AI proposes code changes (via the `write_file` tool) or command executions (via `run_shell_command`), these are treated as high-risk state mutations. The CLI pauses the execution loop, visually alerts the user, and requires explicit Human-in-the-Loop (HITL) authorization (`Y/N/Modify`). **The AI cannot silently mutate local CUI or system state.**

### 3.3 In-Memory Lifecycle & Cryptographic Transport (CRUD)

Because CUI cannot leak to unauthorized persistent storage, the standard Create, Read, Update, Delete (CRUD) lifecycle is heavily biased toward "Delete" and strictly gated "Create" policies.

* **Create (Memory Allocation):** Context payloads are built ephemerally in the workstation's RAM during the `EVALUATING` state of the REPL. The CLI packages the user prompt and approved local file contents into a strictly typed JSON payload required by the Vertex AI API.
* **Read (Cryptographic Transit):** `aegis-cli` transmits the payload to the private Google Cloud endpoint. This transit is secured via FIPS 140-2 validated TLS 1.3. The CLI enforces certificate pinning or strict CA validation to prevent corporate middlebox interception unless explicitly configured by an enterprise administrator.
* **Update (Local Execution):** Updates to the local file system are executed natively via Node.js `fs` modules, but only post-HITL approval.
* **Delete (Zero-Retention & Garbage Collection):** * *At the Edge:* The Node.js V8 engine's garbage collector reclaims the memory used for the context payload once the API request completes. When the user types `/clear` or exits the terminal session (`Ctrl+C`), the process dies, and all active CUI context in RAM is instantly destroyed by the operating system.
* *At the Boundary:* Google Cloud Assured Workloads enforces a strict **Zero-Retention and No-Training** policy for Vertex AI. Payloads are processed in memory for inference, the output is streamed back, and the inbound payload is immediately dropped. It is never cached, logged, or used to fine-tune foundation models.

### 3.4 The Immutable Audit Ledger

To satisfy NIST SP 800-171 Audit and Accountability (3.3.x) controls, Aegis maintains a strict separation between payload data (CUI) and telemetry/audit data.

* **Local Edge Ledger (`~/.aegis/logs/*.jsonl`):** A highly structured, auto-rotating JSON Lines file. It records *metadata only*.
* *What is logged:* Session start/stop times, authenticated user identity, operational mode, paths of files read (e.g., `read_file("src/auth.ts")`), token counts, and exact timestamps of HITL approvals.
* *What is NOT logged:* User prompts, file contents, AI responses, or shell command standard output (`stdout`/`stderr`). **No CUI is ever written to the local audit ledger.**
* **Cloud Boundary Ledger:** GCP Cloud Audit Logs capture `ADMIN_READ`, `DATA_READ`, and `DATA_WRITE` actions on the Vertex AI endpoint. These logs prove that only authorized IAM identities accessed the model from authorized IPs (via VPC-SC) and are stored in a CMEK-encrypted, versioned Cloud Storage bucket.

---

## 4. Google Cloud Infrastructure (Pulumi TypeScript)

A core tenet of `aegis-cli` is that a secure client is useless without a provably secure backend. For the "Self-Service BYOC" and "Enterprise" modes, the Google Cloud environment must be deterministically configured to satisfy NIST SP 800-171, CMMC Level 2, and DoD IL4/IL5 data residency and access requirements.

To eliminate configuration drift and manual setup errors, Aegis does not rely on users clicking through the GCP Console. Instead, the exact compliance boundary is defined as Infrastructure as Code (IaC) using Pulumi TypeScript, which is natively embedded into the CLI.

### 4.1 Embedded IaC via Pulumi Automation API

Aegis abstracts the complexity of infrastructure deployment away from the defense engineer. The CLI utilizes the `@pulumi/pulumi/automation` package, which embeds the Pulumi engine directly within the Node.js process.

* **Zero-Dependency Provisioning:** The developer does not need to install the Pulumi CLI, learn Pulumi CLI commands, or manage external state backends. `aegis init` handles the lifecycle (`preview`, `up`, `destroy`) programmatically.
* **State Management:** For Self-Service mode, the Pulumi state file—which maps the deployed resources—is stored locally in `~/.aegis/state` or securely bootstrapped into a dedicated, locked-down Google Cloud Storage bucket within the user's project.
* **Dynamic Configuration Binding:** Upon a successful `stack.up()` execution, the Automation API returns the infrastructure outputs (VPC name, KMS Key IDs, Endpoint URLs). The CLI immediately intercepts these outputs and writes them to the local `~/.aegis/config.yaml`, ensuring the local agent is instantly hard-wired to the newly provisioned secure boundary.

### 4.2 The Assured Workloads Blueprint (Resource Definitions)

When `aegis init` deploys the backend, it provisions a specific architecture designed to prevent data exfiltration and enforce cryptographic control. The Pulumi TypeScript program defines the following resource stack:

#### 4.2.1 Cryptographic Foundation (Cloud KMS)

To satisfy NIST 800-171 control 3.13.11 (FIPS-validated cryptography), all data at rest within the boundary must be encrypted using Customer-Managed Encryption Keys (CMEK).

* **Resource:** `gcp.kms.KeyRing` and `gcp.kms.CryptoKey`.
* **Configuration:** Keys are provisioned in a designated US Assured Workloads region (e.g., `us-central1`). The `CryptoKey` is configured with a strict 30-day automatic `rotationPeriod`, and the Pulumi resource is marked with `protect: true` to prevent accidental deletion of the key material, which would effectively crypto-shred the audit logs.

#### 4.2.2 Network Isolation & Data Perimeters (VPC & VPC-SC)

To satisfy control 3.1.3 (Control the flow of CUI), network traffic between the developer's workstation and Vertex AI must not traverse the public internet or be accessible from unauthorized IP space.

* **Resource:** `gcp.compute.Network` and `gcp.compute.Subnetwork`.
* **Configuration:** A custom Virtual Private Cloud (VPC) is created with `privateIpGoogleAccess: true` (Private Google Access). This allows the local CLI (if routed via VPN/Interconnect) or internal corporate gateways to hit the Vertex AI APIs using internal Google IP ranges.
* **VPC Service Controls (VPC-SC):** The Pulumi script configures a `gcp.accesscontextmanager.ServicePerimeter`. This places a hard API firewall around `aiplatform.googleapis.com`. Even if an engineer's `gcloud` credentials are stolen, the API will reject any request to the Aegis models that does not originate from an authorized corporate IP address or a verified managed device.

#### 4.2.3 Audit Logging & Accountability (Cloud Storage & IAM)

To satisfy controls 3.3.1 and 3.3.2 (Audit Log Generation), the system must generate immutable records of all AI interactions.

* **Resource:** `gcp.projects.IAMAuditConfig` and `gcp.storage.Bucket`.
* **Configuration:** The Pulumi script modifies the GCP Project's IAM configuration to explicitly enable `ADMIN_READ`, `DATA_READ`, and `DATA_WRITE` audit logs for the `aiplatform.googleapis.com` service.
* **Storage Vault:** A Cloud Storage Bucket is provisioned to receive these routed logs. The bucket is configured with `uniformBucketLevelAccess: true`, versioning enabled, and is encrypted at rest using the CMEK key generated in step 4.2.1. A lifecycle rule is applied to retain logs for exactly 365 days before automatic deletion, balancing compliance with data minimization.

### 4.3 Vertex AI Endpoint Pinning

Defense environments require deterministic model behavior. An AI assistant cannot silently upgrade to a new model weights version overnight, as this invalidates security testing and expected behavior.

* **Explicit Versioning:** The Pulumi script does not deploy against a generic `gemini-1.5-pro` alias. It queries and pins the environment to a specific, vetted model version (e.g., `gemini-1.5-pro-001`).
* **Regional Pinning:** The endpoint is hardcoded to an Assured Workloads region. The `aegis-cli` configuration is generated to strictly route payloads to `https://us-central1-aiplatform.googleapis.com/v1/...` ensuring that no CUI is accidentally routed to a global or out-of-bounds geographic endpoint.

---

## 5. The Agentic Loop & Security Boundary

The defining feature of an agentic coding assistant is its ability to autonomously gather context and execute actions. However, in a DoD or DIB environment, unconstrained autonomy is a critical security vulnerability.

The `aegis-cli` resolves this tension by implementing a strictly gated **Read-Evaluate-Act (REA) Loop** powered by Vertex AI Function Calling. The model evaluates the user's request, but the local Node.js process acts as a rigid, cryptographic bouncer that explicitly authorizes or denies every action the model attempts to take.

### 5.1 The Read-Evaluate-Act (REA) State Machine

When a developer issues a prompt (e.g., `aegis "Fix the failing tests in the auth module"`), the CLI enters the REA loop. This loop runs continuously until the agent determines it has fully resolved the prompt.

1. **Read (Context Assembly):** The CLI bundles the user prompt, the current conversation history, and the JSON schema of available local tools into a payload. It calculates the token count locally (using a lightweight tokenizer) to ensure the payload does not exceed Vertex AI limits or local CUI egress policies.
2. **Evaluate (Cloud Inference):** The payload is transmitted via TLS 1.3 to the private Vertex AI endpoint. The Gemini model evaluates the context. If it needs more information (e.g., "I need to see `src/auth.spec.ts`") or wants to take an action (e.g., "I need to run `npm test`"), it returns a specific `functionCall` payload rather than plain text.
3. **Act (The Local Boundary):** The `aegis-cli` intercepts the `functionCall` response. It does *not* blindly execute it. It routes the request through the Local Tool Schema and the Human-in-the-Loop (HITL) gate.
4. **Inject:** The result of the local action (file contents, stdout, stderr, or a "permission denied" message) is injected back into the conversation history, and the loop returns to step 1.

### 5.2 The Local Tool Schema

The agent is intentionally blind and paralyzed by default. It can only interact with the developer's workstation through explicitly defined TypeScript functions. These tools are categorized by their risk profile:

#### 5.2.1 Safe Tools (Auto-Execute)

These tools perform read-only operations. They do not mutate the local file system or system state.

* `read_file(path: string)`: Reads a file's contents into memory. **Constraint:** Strictly validated against the `.aegisignore` blocklist. If the path is blocked, the CLI returns `{ error: "File access denied by .aegisignore policy" }` to the model.
* `list_directory(path: string)`: Returns the file tree of a given directory to help the agent navigate the repository.
* `get_git_status()`: Executes `git status` and `git diff` to understand what changes the developer has already staged or modified.

#### 5.2.2 State-Mutating Tools (HITL Required)

These tools alter the host machine. They represent the highest risk for destructive actions or unauthorized CUI generation.

* `write_file(path: string, content: string)`: Creates or overwrites a file with new code proposed by the AI.
* `run_shell_command(command: string)`: Executes a shell command (e.g., `npm install`, `pytest`, `cargo build`) and captures the output.

### 5.3 Human-in-the-Loop (HITL) Enforcement

To satisfy CMMC Level 2 requirements for change management and authorized execution, the CLI enforces a strict Human-in-the-Loop (HITL) boundary for all state-mutating tools.

When the REA loop intercepts a call to `write_file` or `run_shell_command`, the Node.js event loop essentially pauses.

1. **Visual Interruption:** The reactive terminal UI visually breaks the flow. The text color changes (e.g., bold yellow or red) to signify an active trust boundary.
2. **Explicit Consent:** The CLI presents a prompt detailing the exact command or file diff the agent intends to execute:
> **⚠️ Agent Action Required**
> Aegis wants to execute: `npm install bcrypt`
> `[ Y ] Approve / [ N ] Deny / [ M ] Modify`


3. **The "Modify" Escape Hatch:** Developers must have the ability to correct a hallucinated or slightly incorrect command without restarting the entire prompt loop. Selecting "Modify" drops the user into an editable text field to fix the command before execution.
4. **Cryptographic Ledgering:** The moment the developer presses `Y` (or submits a modified command), the exact timestamp, user identity, and executed command are immediately written to the local immutable `.jsonl` audit ledger.

By enforcing this strict boundary, `aegis-cli` ensures that the AI is purely an advisor and context-gatherer; the human engineer remains the sole authorizing entity for all state changes on the workstation.

---

## 6. UI/UX: The Reactive Terminal (`ink`)

A common failure point of terminal-based AI assistants is visual fatigue. Traditional command-line interfaces rely on procedural, blocking `stdout` streams. When an agent enters a complex loop—reading multiple files, executing commands, and streaming long markdown responses—a procedural terminal quickly devolves into an unreadable, scrolling wall of text.

For `aegis-cli` to succeed as a first-class pair programmer, the interface must be fluid, non-blocking, and highly organized. To achieve this, Aegis abandons raw `stdout` in favor of **`ink`**, a React-based renderer for the terminal. This allows us to treat the terminal window as a dynamic browser canvas, where components mount, update, and unmount based on the asynchronous state of the agent and the local filesystem.

### 6.1 Layout Architecture (The View)

The `ink` interface is divided into four distinct, state-driven panes. This layout ensures the developer maintains absolute situational awareness of what the agent is doing and, crucially for CUI environments, *what data the agent is seeing*.

#### 6.1.1 The Header (Status Bar)

A persistent top bar that provides immediate environmental context.

* **Left:** Current Working Directory (CWD) and active Git branch (e.g., `~/defense-core/auth ⎇ feature-login`).
* **Center:** The active AI backend and enforcement boundary (e.g., `🛡️ Vertex AI: Gemini 1.5 Pro (us-central1)`).
* **Right (Future-Proofing):** Reserved for decentralized RTMX Sync mesh status (e.g., `🟢 3 Peers | CI: Passing`).

#### 6.1.2 The Context Pane (Transparency & Trust)

When handling CUI, the developer must never guess what data is in the AI's context window.

* This pane acts as a live, updating sidebar or footer.
* It lists every file currently loaded into the agent's memory via the `read_file` tool (e.g., `📄 src/auth.ts (4KB)`).
* It displays a live **Token Utilization Meter** (e.g., `Tokens: 14,205 / 128,000`). If the context window approaches a predefined threshold, the meter changes color (yellow/red), prompting the user to prune unnecessary context.

#### 6.1.3 The Main Chat Log (The Scroll)

This is the historical record of the current session.

* **Model Output:** Markdown responses from Gemini are streamed chunk-by-chunk. Code blocks are automatically parsed and syntax-highlighted based on the language tag.
* **Collapsed Actions:** To prevent clutter, when the agent executes a safe tool (like `list_directory`), the UI renders a transient spinner. Once complete, the spinner collapses into a single, unobtrusive line (e.g., `✓ Navigated to /src/components`).

#### 6.1.4 The REPL Input (The Prompt)

A robust, multi-line input field anchored to the bottom of the screen. It supports standard terminal keystrokes (up/down for history) and allows engineers to paste large chunks of code or logs without triggering premature submission.

### 6.2 Visualizing the Agentic Loop

The UI is deeply coupled to the Read-Evaluate-Act (REA) state machine defined in Section 5. The interface reacts instantly to changes in the underlying state:

1. **State: `EVALUATING`:** The REPL input locks. A `Thinking...` component mounts in the Main Chat Log, providing visual feedback that the Vertex AI API is processing the payload.
2. **State: `TOOL_EXECUTION` (Safe):** The `Thinking` component updates to reflect the specific tool being used (e.g., `⠋ Reading src/utils.ts...`).
3. **State: `ACTION_REQUIRED` (HITL Boundary):** The UI drastically shifts focus. The standard text colors dim, and a brightly colored (e.g., bold yellow) `@clack/prompts`-style interactive component mounts. This breaks the visual monotony, demanding the developer's attention to authorize a state-mutating command (`write_file` or `run_shell_command`).
4. **State: `STREAMING`:** The tool execution components unmount, and a Markdown renderer component mounts, providing a satisfying, typewriter-like stream of the final proposed solution or code fix.

### 6.3 Ergonomics & Slash Commands

A frictionless developer experience means never having to leave the active REPL session to configure the tool or manage context. Aegis implements standard slash commands that are natively auto-completed in the input field:

* `/clear`: Wipes the active conversation history and empties the Context Pane, immediately destroying the in-memory CUI payload.
* `/add <path>`: Forces a specific file or directory into the Context Pane without waiting for the agent to request it.
* `/drop <path>`: Explicitly removes a file from the AI's context window to free up tokens or remove irrelevant data.
* `/ignore <path>`: Appends the specified path to the `.aegisignore` file on the fly, permanently blinding the agent to that resource for the current and future sessions.
* `/mode <target>`: Hot-swaps the active backend (e.g., switching from `vertex-gcp` to `local-edge-llama3`) updating the Header and routing state machine instantly.

---

## 7. Auditing & Body of Evidence

In the Defense Industrial Base (DIB), a system is only as secure as its audit trail. If a defense contractor cannot cryptographically or deterministically prove that Controlled Unclassified Information (CUI) was handled correctly, the system fails the compliance audit—regardless of its actual technical security.

Because `aegis-cli` introduces an autonomous agent into the developer workflow, it must provide a pristine, irrefutable **Body of Evidence** that maps every AI action back to explicit human authorization and secure cloud execution.

### 7.1 The Dual-Ledger Architecture

To achieve end-to-end traceability without unnecessarily mirroring CUI across the network, Aegis employs a Dual-Ledger architecture. It records intent and authorization at the **Edge** (the workstation) and API execution at the **Boundary** (the GCP backend).

#### 7.1.1 The Edge Ledger (Local Metadata)

The CLI maintains an immutable, local ledger in the user's home directory (`~/.aegis/logs/*.jsonl`).

* **Format:** JSON Lines (`.jsonl`) is used because it is append-only, highly resilient to process crashes, and natively ingestible by enterprise SIEMs (like Splunk, Elastic, or Datadog) if the organization mandates endpoint log forwarding.
* **The "No Payload" Rule:** The local ledger records *metadata only*. It explicitly strips all prompt text, CUI file contents, and shell stdout/stderr.
* **Logged Events:**
* `SESSION_START` / `SESSION_END`: Timestamps, user identity (via OS or GCP ADC), and the active Aegis mode.
* `CONTEXT_READ`: The exact file path accessed via `read_file` (e.g., `src/auth/token.ts`), its byte size, and the calculated token count.
* `HITL_AUTHORIZATION`: The exact shell command or file diff the AI proposed, the timestamp it was proposed, and the exact timestamp and boolean result of the developer's `Y/N` terminal input.


* **Automated Pruning:** To prevent disk bloat and adhere to data minimization principles, local logs automatically rotate and prune data older than 90 days.

#### 7.1.2 The Boundary Ledger (Cloud Source of Truth)

While the Edge Ledger proves what the developer authorized, the Boundary Ledger proves what the Vertex AI endpoint actually processed, ensuring no unauthorized exfiltration occurred.

* **Data Access Logs:** As configured by the Pulumi backend (Section 4), GCP Cloud Audit Logs automatically capture all `ADMIN_READ`, `DATA_READ`, and `DATA_WRITE` operations against `aiplatform.googleapis.com`.
* **Immutability & Encryption:** These logs are routed into a locked Google Cloud Storage bucket. They are encrypted at rest via Customer-Managed Encryption Keys (CMEK) and protected by a bucket retention policy that prevents deletion or modification by any user—even project administrators—for a defined period (e.g., 365 days).

### 7.2 Generating the Body of Evidence (`aegis audit export`)

Compliance officers and auditors do not want to manually stitch together JSON files and cloud logs. To make the developer and security experience truly first-class, Aegis includes a built-in compliance reporting engine.

Developers or security personnel can run:
`aegis audit export --start 2026-02-01 --end 2026-02-28 --format pdf --correlate`

* **Correlation Engine:** If the `--correlate` flag is passed and the user has sufficient GCP IAM permissions, the CLI queries the GCP Cloud Logging API. It matches the local `HITL_AUTHORIZATION` timestamps with the corresponding GCP `DATA_WRITE` API execution timestamps.
* **The Output Report:** The CLI generates a human-readable PDF or CSV report. It translates the raw JSON into a plain-English compliance narrative:
> *"Between Feb 1 and Feb 28, User Alice initiated 142 Aegis sessions. The agent requested access to 84 local files. Alice explicitly authorized 47 state-mutating shell commands. 1.2M tokens were transmitted to the US-Central1 Assured Workloads boundary via TLS 1.3. 0 policy violations occurred."*

### 7.3 Telemetry & Crash Reporting (The Airgap Rule)

Software needs telemetry to improve, but CUI environments strictly forbid unauthorized data egress. `aegis-cli` implements a strict "Airgap Rule" for operational telemetry (e.g., error rates, latency metrics, crash dumps).

* **Logical Separation:** The code pathways that handle CUI context generation are physically isolated from the telemetry emitter.
* **Sanitization:** If the `ink` UI crashes while rendering a 4MB file, the crash dump reports the stack trace and the error class (`OutOfMemoryError`), but aggressively sanitizes all string variables to ensure no source code is leaked into the crash report.
* **Opt-In by Default:** For `self-service` and `enterprise` modes, all external telemetry is disabled by default. It must be explicitly enabled via `aegis config set telemetry true` by the user or organizational policy.

---

# Minimum Viable Product (MVP) Definition of Done (DoD)

The MVP for `aegis-cli` (v1.0.0) is considered "Done" when a defense engineer can install the package via `npm`, successfully provision their own compliant Google Cloud backend using the CLI, and safely use the agent to modify local code with explicit HITL safeguards and complete audit logging.

**Core MVP Capabilities:**

* **Infrastructure Automation:** `aegis init` successfully executes the embedded Pulumi Automation API to deploy a standard GCP Assured Workloads configuration (VPC-SC, KMS, Audit Logging) for the "Self-Service BYOC" path.
* **Agentic Loop:** The CLI can process multi-line prompts, utilize Vertex AI function calling, and execute at least two local tools (`read_file` and `run_shell_command`).
* **HITL Enforcement:** The system effectively pauses and mandates explicit developer consent (`Y/N`) before executing any state-mutating local commands.
* **Reactive UI:** The terminal interface utilizes `ink` to provide streaming Markdown output, a context-aware header, and clear visual delineation between "thinking/acting" and "responding."
* **Dual-Ledger Auditing:** The system successfully writes local `.jsonl` metadata logs without spilling CUI payloads, and the GCP backend successfully records Data Access logs.

## Acceptance Criteria (AC)

* **AC1 (Data Leakage Prevention):** Code inspections confirm that CUI payloads and AI prompt contents are never written to local disk storage, residing only in transient RAM during the active session.
* **AC2 (Boundary Enforcement):** Network traces verify that `aegis-cli` communicates *only* with the designated private GCP endpoints configured during `init`, and all traffic utilizes FIPS-140-2 validated TLS 1.3.
* **AC3 (Context Gating):** The `read_file` tool strictly honors the `.aegisignore` file, throwing a localized permission error if the AI attempts to read a restricted file (e.g., `.env` or AWS credential files).
* **AC4 (Traceability):** An engineer can match a specific local `.jsonl` action record (e.g., execution of `npm test`) to the corresponding GCP Cloud Audit Log timestamp, proving end-to-end traceability.

## Notional Phasing

To manage engineering risk, the project will be executed in the following phases:

* **Phase 1: Proof of Concept (PoC) Foundation:** Establish the `ink` React terminal loop and basic, hardcoded Vertex AI streaming communication.
* **Phase 2: The Constrained Agent:** Implement local tool execution (`read_file`, `run_shell_command`), `.aegisignore` filtering, and the core Human-in-the-Loop (HITL) state machine.
* **Phase 3: Infrastructure as Code (IaC) Integration:** Embed the Pulumi TypeScript models and build the `aegis init` self-service deployment flow.
* **Phase 4: Auditing & MVP Release:** Finalize the dual-ledger logging architecture, polish the UX, and publish v1.0.0 to npm.
* **Phase 5 (Post-MVP): RTMX Edge Mesh:** Introduce RTMX Sync capabilities, peer-to-peer context sharing, and support for swapping the Vertex AI backend with local, edge-hosted models (e.g., Llama 3) for fully air-gapped environments.

---

