/**
 * LegalPage - Full-screen overlay for Terms of Service and Privacy Policy
 *
 * Features:
 * - Tab-based navigation (Terms of Service, Privacy Policy)
 * - Static hardcoded legal content (no API calls)
 * - Keyboard shortcuts (Escape to close)
 * - Click outside to close
 * - URL push state for deep linking (/terms, /privacy)
 */

// ============================================================================
// Types
// ============================================================================

export type LegalTab = 'terms' | 'privacy'

export interface LegalPageOptions {
  onClose?: () => void
}

// ============================================================================
// Constants
// ============================================================================

const TABS: { id: LegalTab; label: string }[] = [
  { id: 'terms', label: 'Terms of Service' },
  { id: 'privacy', label: 'Privacy Policy' },
]

const LEGAL_ICON_SVG = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/><polyline points="10 9 9 9 8 9"/></svg>`

// ============================================================================
// LegalPage Class
// ============================================================================

export class LegalPage {
  private container: HTMLElement | null = null
  private currentTab: LegalTab = 'terms'
  private keydownHandler: ((e: KeyboardEvent) => void) | null = null
  private options: LegalPageOptions

  constructor(options: LegalPageOptions = {}) {
    this.options = options
    this.createContainer()
    this.setupKeyboardShortcuts()
  }

  // ==========================================================================
  // Container Setup
  // ==========================================================================

  /**
   * Create the legal page container and all UI elements
   */
  private createContainer(): void {
    const container = document.createElement('div')
    container.className = 'legal-page'

    container.innerHTML = `
      <div class="legal-container">
        <div class="legal-header">
          <div class="legal-heading">
            <div class="legal-title">
              <span class="legal-title-icon">${LEGAL_ICON_SVG}</span>
              <span>Legal</span>
            </div>
            <p class="legal-summary">
              Review what you agree to and how your workspace data is handled.
            </p>
          </div>
          <button class="legal-close" title="Close">&times;</button>
        </div>
        <div class="legal-tabs">
          ${TABS.map(
            (tab) => `
            <button class="legal-tab${tab.id === this.currentTab ? ' active' : ''}" data-tab="${tab.id}">
              ${tab.label}
            </button>
          `
          ).join('')}
        </div>
        <div class="legal-content">
          <!-- Tab content rendered here -->
        </div>
      </div>
    `

    document.body.appendChild(container)
    this.container = container

    // Close button event listener
    container.querySelector('.legal-close')?.addEventListener('click', () => this.hide())

    // Click outside to close
    container.addEventListener('click', (e) => {
      if (e.target === container) this.hide()
    })

    // Tab switching event listeners
    container.querySelectorAll('.legal-tab').forEach((tab) => {
      tab.addEventListener('click', () => {
        const tabId = tab.getAttribute('data-tab') as LegalTab
        this.switchTab(tabId)
      })
    })
  }

  /**
   * Setup keyboard shortcuts for the legal page
   */
  private setupKeyboardShortcuts(): void {
    this.keydownHandler = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && this.isVisible()) {
        this.hide()
        e.preventDefault()
      }
    }

    document.addEventListener('keydown', this.keydownHandler)
  }

  // ==========================================================================
  // Tab Switching
  // ==========================================================================

  /**
   * Switch to a different tab
   */
  private switchTab(tab: LegalTab): void {
    if (tab === this.currentTab) return

    this.currentTab = tab

    // Update tab active state
    this.container?.querySelectorAll('.legal-tab').forEach((t) => {
      t.classList.toggle('active', t.getAttribute('data-tab') === tab)
    })

    // Update URL
    const path = tab === 'terms' ? '/terms' : '/privacy'
    history.pushState(null, '', path)

    // Render content
    this.renderContent()
  }

  /**
   * Render the current tab content
   */
  private renderContent(): void {
    const content = this.container?.querySelector('.legal-content') as HTMLElement
    if (!content) return

    content.innerHTML = this.currentTab === 'terms' ? this.renderTerms() : this.renderPrivacy()

    // Scroll to top when switching tabs
    content.scrollTop = 0
  }

  // ==========================================================================
  // Lifecycle Methods
  // ==========================================================================

  /**
   * Show the legal page
   */
  show(tab?: LegalTab): void {
    if (tab) {
      this.currentTab = tab
      // Update tab active state
      this.container?.querySelectorAll('.legal-tab').forEach((t) => {
        t.classList.toggle('active', t.getAttribute('data-tab') === tab)
      })
    }

    // Update URL
    const path = this.currentTab === 'terms' ? '/terms' : '/privacy'
    history.pushState(null, '', path)

    this.container?.classList.add('visible')
    this.renderContent()
  }

  /**
   * Hide the legal page
   */
  hide(): void {
    this.container?.classList.remove('visible')
    this.options.onClose?.()
  }

  /**
   * Check if the legal page is currently visible
   */
  isVisible(): boolean {
    return this.container?.classList.contains('visible') ?? false
  }

  /**
   * Clean up the legal page and remove from DOM
   */
  destroy(): void {
    if (this.keydownHandler) {
      document.removeEventListener('keydown', this.keydownHandler)
      this.keydownHandler = null
    }

    this.container?.remove()
    this.container = null
  }

  // ==========================================================================
  // Terms of Service Content
  // ==========================================================================

  /**
   * Render the Terms of Service content
   */
  private renderTerms(): string {
    return `
      <div class="legal-effective-date">Effective Date: February 1, 2026</div>

      <div class="legal-section">
        <h2 class="legal-section-title">1. Acceptance of Terms</h2>
        <p class="legal-text">
          Welcome to Wisdoverse Forge, a product of Wisdoverse Forge ("Company," "we," "us," or "our"). By accessing or using
          the Wisdoverse Forge platform, including all associated websites, applications, APIs, and services (collectively,
          the "Service"), you agree to be bound by these Terms of Service ("Terms"). If you do not agree to all of
          these Terms, you may not access or use the Service.
        </p>
        <p class="legal-text">
          These Terms constitute a legally binding agreement between you and Wisdoverse Forge. By creating an account,
          accessing the Service, or otherwise indicating your acceptance, you represent that you have the legal capacity
          to enter into this agreement. If you are using the Service on behalf of an organization, you represent and
          warrant that you have the authority to bind that organization to these Terms.
        </p>
      </div>

      <div class="legal-section">
        <h2 class="legal-section-title">2. Description of Service</h2>
        <p class="legal-text">
          Wisdoverse Forge is a self-hosted governed AI workbench for teams. The Service provides:
        </p>
        <ul class="legal-list">
          <li>Agent management for creating, starting, stopping, and reviewing managed AI agents</li>
          <li>Task boards and work history so teams can track what each agent is doing</li>
          <li>Result records, saved notes, and saved instructions that help you understand agent results</li>
          <li>Team, project, and workspace controls for keeping work separated by organization</li>
          <li>Connections to supported AI services and work tools chosen by your organization</li>
          <li>Live activity updates and notifications for important task and agent changes</li>
          <li>Operator tools for setup, troubleshooting, and supported automation</li>
        </ul>
        <p class="legal-text">
          We reserve the right to modify, suspend, or discontinue any part of the Service at any time, with or without
          notice. We will make reasonable efforts to notify users of significant changes in advance.
        </p>
      </div>

      <div class="legal-section">
        <h2 class="legal-section-title">3. User Accounts & Registration</h2>
        <p class="legal-text">
          To access certain features of the Service, you must create an account. When registering, you agree to:
        </p>
        <ul class="legal-list">
          <li>Provide accurate, current, and complete information during the registration process</li>
          <li>Maintain and promptly update your account information to keep it accurate and complete</li>
          <li>Maintain the security and confidentiality of your login credentials, including your password and any access keys</li>
          <li>Accept responsibility for all activities that occur under your account</li>
          <li>Notify us immediately of any unauthorized use of your account or any other breach of security</li>
        </ul>
        <p class="legal-text">
          Passwords must meet our security requirements, which include a minimum of 12 characters containing uppercase
          and lowercase letters, numbers, and special characters, in accordance with NIST SP 800-63B guidelines. You may
          also sign in through supported third-party providers, such as GitHub or Google, when your organization enables
          that option.
        </p>
        <p class="legal-text">
          We reserve the right to suspend or terminate accounts that violate these Terms, that have been inactive for an
          extended period, or that we reasonably believe are being used fraudulently.
        </p>
      </div>

      <div class="legal-section">
        <h2 class="legal-section-title">4. Acceptable Use</h2>
        <p class="legal-text">
          You agree to use the Service only for lawful purposes and in accordance with these Terms. You agree not to:
        </p>
        <ul class="legal-list">
          <li>Reverse engineer, decompile, disassemble, or otherwise attempt to derive the source code of the Service, except to the extent expressly permitted by applicable law</li>
          <li>Use the Service to transmit, distribute, or store material that violates any applicable law, regulation, or third-party rights</li>
          <li>Attempt to gain unauthorized access to the Service, other accounts, computer systems, or networks connected to the Service</li>
          <li>Use the Service to engage in any form of automated data collection (scraping, crawling, or harvesting) without our prior written consent</li>
          <li>Interfere with or disrupt the integrity or performance of the Service, including overloading servers, introducing malware, or conducting denial-of-service attacks</li>
          <li>Use the Service to send unsolicited communications, spam, or phishing messages</li>
          <li>Resell, sublicense, or redistribute access to the Service without our express written authorization</li>
          <li>Circumvent or disable any security features, rate limits, access controls, or usage restrictions of the Service</li>
          <li>Use the Service in any manner that could damage, disable, overburden, or impair the Service or interfere with any other party's use of the Service</li>
          <li>Use the Service to develop a competing product or service</li>
        </ul>
        <p class="legal-text">
          Violation of these acceptable use provisions may result in immediate suspension or termination of your account
          and access to the Service, without prior notice or liability.
        </p>
      </div>

      <div class="legal-section">
        <h2 class="legal-section-title">5. Intellectual Property</h2>
        <p class="legal-text">
          The Service, including all software, design, text, graphics, interfaces, and the selection and arrangement
          thereof, is owned by Wisdoverse Forge and is protected by intellectual property laws. Our trademarks, service
          marks, logos, and trade names may not be used without our prior written consent.
        </p>
        <h3 class="legal-subsection-title">Our Intellectual Property</h3>
        <p class="legal-text">
          All rights, title, and interest in and to the Service, including all associated intellectual property rights,
          are and will remain with Wisdoverse Forge and our licensors. Nothing in these Terms transfers any such rights
          to you. You are granted a limited, non-exclusive, non-transferable, revocable license to use the Service
          solely in accordance with these Terms.
        </p>
        <h3 class="legal-subsection-title">Your Content</h3>
        <p class="legal-text">
          You retain all rights to your code, data, prompts, agent content, and other materials that you submit to
          or create through the Service ("Your Content"). By using the Service, you grant us a limited license to
          process, store, and transmit Your Content solely as necessary to provide the Service to you. We do not claim
          ownership of Your Content.
        </p>
        <h3 class="legal-subsection-title">Feedback</h3>
        <p class="legal-text">
          If you provide us with feedback, suggestions, or ideas regarding the Service ("Feedback"), you grant us a
          worldwide, perpetual, irrevocable, royalty-free license to use and incorporate such Feedback into the Service
          without any obligation or compensation to you.
        </p>
      </div>

      <div class="legal-section">
        <h2 class="legal-section-title">6. Payment & Billing</h2>
        <p class="legal-text">
          Certain features of the Service require a paid subscription. By subscribing to a paid plan, you agree to the
          following:
        </p>
        <h3 class="legal-subsection-title">Payment Processing</h3>
        <p class="legal-text">
          All payments are processed securely through Stripe, Inc. ("Stripe"), our third-party payment processor. By
          providing your payment information, you agree to Stripe's
          <a href="https://stripe.com/legal" target="_blank" rel="noopener">terms of service</a> and
          <a href="https://stripe.com/privacy" target="_blank" rel="noopener">privacy policy</a>. We do not directly
          store your full credit card information on our servers.
        </p>
        <h3 class="legal-subsection-title">Subscription & Auto-Renewal</h3>
        <p class="legal-text">
          Paid subscriptions automatically renew at the end of each billing period (monthly or annually) unless you
          cancel before the renewal date. You will be charged the then-current subscription fee at the start of each
          renewal period. We will make reasonable efforts to notify you of any price changes before they take effect.
        </p>
        <h3 class="legal-subsection-title">Cancellation</h3>
        <p class="legal-text">
          You may cancel your subscription at any time through your account settings or by contacting us. Upon
          cancellation, you will retain access to paid features until the end of your current billing period. After
          that, your account will be downgraded to the free tier.
        </p>
        <h3 class="legal-subsection-title">Refunds</h3>
        <p class="legal-text">
          We offer a full refund within 14 days of your initial subscription purchase or an upgrade, provided you have
          not substantially used the paid features during that period. Refund requests should be directed to
          <a href="mailto:legal@wisdoverse.com">legal@wisdoverse.com</a>. Refunds for renewals are evaluated on a case-by-case basis.
        </p>
        <h3 class="legal-subsection-title">Taxes</h3>
        <p class="legal-text">
          Subscription fees are exclusive of applicable taxes unless otherwise stated. You are responsible for all
          applicable taxes, and we will charge tax where required by law.
        </p>
      </div>

      <div class="legal-section">
        <h2 class="legal-section-title">7. Data & Privacy</h2>
        <p class="legal-text">
          Your privacy is important to us. Our collection, use, and protection of your personal information is governed
          by our <a href="/privacy" class="legal-link">Privacy Policy</a>, which is incorporated into these Terms by
          reference. By using the Service, you consent to the data practices described in our Privacy Policy.
        </p>
        <p class="legal-text">
          We are committed to safeguarding your data through industry-standard security measures, including encryption
          at rest, encryption in transit, secure password hashing, and regular security audits.
        </p>
      </div>

      <div class="legal-section">
        <h2 class="legal-section-title">8. Service Availability</h2>
        <p class="legal-text">
          We strive to maintain high availability of the Service, but we do not guarantee uninterrupted or error-free
          operation. The Service is provided on a "best-effort" basis.
        </p>
        <ul class="legal-list">
          <li>We may perform scheduled maintenance during off-peak hours and will make reasonable efforts to provide advance notice of planned downtime</li>
          <li>Emergency maintenance or unplanned outages may occur without prior notice</li>
          <li>We are not liable for any loss of data, revenue, or business interruption resulting from Service downtime or interruptions</li>
          <li>We do not provide a formal service-level agreement (SLA) for the free tier; paid plans may include SLA commitments as specified in your subscription terms</li>
        </ul>
        <p class="legal-text">
          We reserve the right to temporarily restrict access to the Service for maintenance, upgrades, or security
          purposes. We will use commercially reasonable efforts to minimize the duration and impact of any such
          restrictions.
        </p>
      </div>

      <div class="legal-section">
        <h2 class="legal-section-title">9. Limitation of Liability</h2>
        <div class="legal-highlight">
          <p class="legal-text">
            TO THE MAXIMUM EXTENT PERMITTED BY APPLICABLE LAW, IN NO EVENT SHALL AGENTFORGE, ITS DIRECTORS,
            EMPLOYEES, PARTNERS, AGENTS, SUPPLIERS, OR AFFILIATES BE LIABLE FOR ANY INDIRECT, INCIDENTAL, SPECIAL,
            CONSEQUENTIAL, OR PUNITIVE DAMAGES, INCLUDING WITHOUT LIMITATION, LOSS OF PROFITS, DATA, USE, GOODWILL,
            OR OTHER INTANGIBLE LOSSES, RESULTING FROM:
          </p>
          <ul class="legal-list">
            <li>YOUR ACCESS TO OR USE OF, OR INABILITY TO ACCESS OR USE, THE SERVICE;</li>
            <li>ANY CONDUCT OR CONTENT OF ANY THIRD PARTY ON THE SERVICE;</li>
            <li>ANY CONTENT OBTAINED FROM THE SERVICE;</li>
            <li>UNAUTHORIZED ACCESS, USE, OR ALTERATION OF YOUR TRANSMISSIONS OR CONTENT;</li>
            <li>ANY ERRORS, BUGS, OR INACCURACIES IN THE SERVICE.</li>
          </ul>
        </div>
        <p class="legal-text">
          OUR TOTAL AGGREGATE LIABILITY TO YOU FOR ALL CLAIMS ARISING OUT OF OR RELATING TO THESE TERMS OR THE
          SERVICE SHALL NOT EXCEED THE GREATER OF (A) THE AMOUNT YOU HAVE PAID US IN THE TWELVE (12) MONTHS PRECEDING
          THE CLAIM, OR (B) ONE HUNDRED US DOLLARS ($100).
        </p>
        <p class="legal-text">
          Some jurisdictions do not allow the exclusion or limitation of certain damages, so some or all of the above
          limitations may not apply to you. In such cases, our liability will be limited to the fullest extent
          permitted by applicable law.
        </p>
      </div>

      <div class="legal-section">
        <h2 class="legal-section-title">10. Termination</h2>
        <p class="legal-text">
          We may terminate or suspend your account and access to the Service immediately, without prior notice or
          liability, for any reason, including but not limited to:
        </p>
        <ul class="legal-list">
          <li>Breach of these Terms or any applicable policies</li>
          <li>Engaging in fraudulent, abusive, or illegal activity</li>
          <li>Non-payment of subscription fees</li>
          <li>Request by law enforcement or government authority</li>
          <li>Extended periods of account inactivity</li>
          <li>Discontinuation of the Service or any part thereof</li>
        </ul>
        <p class="legal-text">
          Upon termination, your right to use the Service will immediately cease. We will make commercially reasonable
          efforts to allow you to export your data prior to account deletion, subject to the data export provisions in
          our Privacy Policy. Provisions of these Terms that by their nature should survive termination will survive,
          including ownership provisions, warranty disclaimers, indemnity, and limitations of liability.
        </p>
      </div>

      <div class="legal-section">
        <h2 class="legal-section-title">11. Governing Law</h2>
        <p class="legal-text">
          These Terms shall be governed by and construed in accordance with the laws of the People's Republic of China,
          without regard to its conflict of law provisions. Any disputes arising from or relating to these Terms or the
          Service shall be subject to the exclusive jurisdiction of the courts located in Shenzhen, Guangdong Province,
          People's Republic of China.
        </p>
        <p class="legal-text">
          You agree that any cause of action arising out of or related to the Service must commence within one (1) year
          after the cause of action accrues; otherwise, such cause of action is permanently barred.
        </p>
      </div>

      <div class="legal-section">
        <h2 class="legal-section-title">12. Changes to Terms</h2>
        <p class="legal-text">
          We reserve the right to modify these Terms at any time. When we make changes, we will update the "Effective
          Date" at the top of this document and, for material changes, we will provide prominent notice through the
          Service or via email to the address associated with your account.
        </p>
        <p class="legal-text">
          Your continued use of the Service after the effective date of any modifications constitutes your acceptance of
          the updated Terms. If you do not agree to the modified Terms, you must stop using the Service and may request
          deletion of your account.
        </p>
      </div>

      <div class="legal-section">
        <h2 class="legal-section-title">13. General Provisions</h2>
        <h3 class="legal-subsection-title">Entire Agreement</h3>
        <p class="legal-text">
          These Terms, together with our Privacy Policy and any other policies referenced herein, constitute the entire
          agreement between you and Wisdoverse Forge regarding the Service and supersede all prior agreements and
          understandings.
        </p>
        <h3 class="legal-subsection-title">Severability</h3>
        <p class="legal-text">
          If any provision of these Terms is found to be unenforceable or invalid, that provision will be limited or
          eliminated to the minimum extent necessary, and the remaining provisions will remain in full force and effect.
        </p>
        <h3 class="legal-subsection-title">Waiver</h3>
        <p class="legal-text">
          Our failure to enforce any right or provision of these Terms will not be considered a waiver of those rights.
        </p>
        <h3 class="legal-subsection-title">Assignment</h3>
        <p class="legal-text">
          You may not assign or transfer these Terms or your rights under them without our prior written consent. We may
          assign our rights and obligations under these Terms without restriction.
        </p>
      </div>

      <div class="legal-section">
        <h2 class="legal-section-title">14. Contact</h2>
        <div class="legal-contact">
          <p class="legal-text">
            If you have any questions, concerns, or requests regarding these Terms of Service, please contact us:
          </p>
          <p class="legal-text">
            <strong>Wisdoverse Forge</strong><br>
            Email: <a href="mailto:legal@wisdoverse.com">legal@wisdoverse.com</a>
          </p>
        </div>
      </div>
    `
  }

  // ==========================================================================
  // Privacy Policy Content
  // ==========================================================================

  /**
   * Render the Privacy Policy content
   */
  private renderPrivacy(): string {
    return `
      <div class="legal-effective-date">Effective Date: February 1, 2026</div>

      <div class="legal-section">
        <h2 class="legal-section-title">1. Information We Collect</h2>
        <p class="legal-text">
          Wisdoverse Forge ("Company," "we," "us," or "our") operates the Wisdoverse Forge platform (the "Service"). This
          Privacy Policy explains how we collect, use, disclose, and safeguard your information when you use our Service.
          We are committed to protecting your privacy and handling your data transparently and responsibly.
        </p>
        <h3 class="legal-subsection-title">Account Information</h3>
        <p class="legal-text">
          When you create an account, we collect information necessary for account management:
        </p>
        <ul class="legal-list">
          <li>Email address (used as your primary identifier and for account communications)</li>
          <li>Display name (shown within the Service interface)</li>
          <li>Password (stored as a protected password hash; we never store plaintext passwords)</li>
          <li>Sign-in provider identifiers (if you sign in through GitHub, Google, or another supported option)</li>
          <li>Organization membership and role information (if applicable)</li>
        </ul>
        <h3 class="legal-subsection-title">Usage Data</h3>
        <p class="legal-text">
          We automatically collect information about how you interact with the Service:
        </p>
        <ul class="legal-list">
          <li>Agent records, including when an agent was created and whether it is ready, working, or unavailable</li>
          <li>Tool activity records, such as which kind of action ran and whether it succeeded</li>
          <li>Project repository details, such as branch names, commit hashes, and file change counts; we do not collect the content of your code files</li>
          <li>Feature usage patterns, such as views opened, settings changed, and features enabled</li>
          <li>Service request records, such as the action requested, status, and time</li>
          <li>Live update connection records, such as connection status and message type</li>
        </ul>
        <h3 class="legal-subsection-title">Device & Technical Data</h3>
        <p class="legal-text">
          We may collect technical information about the devices you use to access the Service:
        </p>
        <ul class="legal-list">
          <li>IP address (used for security, rate limiting, and audit logging)</li>
          <li>Browser type and version</li>
          <li>Operating system</li>
          <li>User-agent string</li>
          <li>Referring URLs</li>
        </ul>
        <h3 class="legal-subsection-title">Payment Information</h3>
        <p class="legal-text">
          If you subscribe to a paid plan, payment information (credit card number, billing address) is collected and
          processed directly by Stripe, Inc. We receive only limited payment metadata (last four digits of your card,
          card brand, expiration date, billing country) from Stripe for record-keeping purposes. We do not store full
          credit card numbers on our servers.
        </p>
      </div>

      <div class="legal-section">
        <h2 class="legal-section-title">2. How We Use Your Information</h2>
        <p class="legal-text">
          We use the information we collect for the following purposes:
        </p>
        <h3 class="legal-subsection-title">Service Delivery</h3>
        <ul class="legal-list">
          <li>To create and manage your account</li>
          <li>To provide the core Wisdoverse Forge agent, task, context, and workspace functionality</li>
          <li>To show live task, agent, and evidence updates in the product interface</li>
          <li>To coordinate team workflows across managed agents</li>
          <li>To authenticate your identity and authorize access to protected resources</li>
          <li>To process payments and manage subscriptions</li>
        </ul>
        <h3 class="legal-subsection-title">Service Improvement</h3>
        <ul class="legal-list">
          <li>To analyze usage patterns and identify areas for improvement</li>
          <li>To monitor and optimize Service performance, reliability, and stability</li>
          <li>To develop new features and functionality based on aggregate usage data</li>
          <li>To detect and diagnose technical issues and bugs</li>
        </ul>
        <h3 class="legal-subsection-title">Analytics</h3>
        <ul class="legal-list">
          <li>To generate aggregate, anonymized statistics about Service usage</li>
          <li>To understand how users interact with different features of the platform</li>
          <li>To measure the effectiveness of interface changes and new features</li>
        </ul>
        <h3 class="legal-subsection-title">Communication & Support</h3>
        <ul class="legal-list">
          <li>To send important account notifications (security alerts, password changes, billing events)</li>
          <li>To respond to your inquiries, support requests, and feedback</li>
          <li>To send Service-related announcements (downtime notifications, feature updates, policy changes)</li>
        </ul>
        <h3 class="legal-subsection-title">Security & Compliance</h3>
        <ul class="legal-list">
          <li>To detect, prevent, and respond to fraud, abuse, security incidents, and other harmful activity</li>
          <li>To enforce our Terms of Service and other applicable policies</li>
          <li>To comply with legal obligations and respond to lawful requests from public authorities</li>
          <li>To maintain audit logs for security monitoring and compliance purposes</li>
        </ul>
      </div>

      <div class="legal-section">
        <h2 class="legal-section-title">3. Data Storage & Security</h2>
        <p class="legal-text">
          We implement industry-standard technical and organizational security measures to protect your personal
          information against unauthorized access, alteration, disclosure, or destruction.
        </p>
        <h3 class="legal-subsection-title">Encryption</h3>
        <ul class="legal-list">
          <li><strong>Stored data:</strong> Data stored by the Service is protected with encryption. Saved access keys receive additional protection.</li>
          <li><strong>Data in transit:</strong> Communications between your browser and our servers are encrypted using secure HTTPS connections. Internal service communication uses protected channels.</li>
        </ul>
        <h3 class="legal-subsection-title">Authentication Security</h3>
        <ul class="legal-list">
          <li>Passwords are stored as protected hashes rather than readable passwords</li>
          <li>Login sessions are signed and expire automatically</li>
          <li>Longer-lived login sessions are rotated to reduce reuse risk</li>
          <li>Repeated failed login attempts may temporarily lock the account</li>
          <li>Repeated sign-in and request attempts may be slowed or blocked to protect the Service</li>
        </ul>
        <h3 class="legal-subsection-title">Infrastructure Security</h3>
        <ul class="legal-list">
          <li>Protective checks that help the Service recover when supporting systems are unhealthy</li>
          <li>Sensitive data filtering in error tracking to reduce accidental exposure of passwords, access keys, and credit card numbers</li>
          <li>Safe database query practices to reduce injection risk</li>
          <li>Browser access policies that limit which sites can call the Service</li>
          <li>Security review supported by audit logging</li>
        </ul>
      </div>

      <div class="legal-section">
        <h2 class="legal-section-title">4. Data Sharing</h2>
        <div class="legal-highlight">
          <p class="legal-text">
            <strong>We do not sell, rent, or trade your personal information to third parties.</strong> We only share
            your data in the following limited circumstances:
          </p>
        </div>
        <h3 class="legal-subsection-title">Service Providers</h3>
        <p class="legal-text">
          We share data with trusted third-party service providers who assist us in operating the Service, subject to
          strict data processing agreements:
        </p>
        <ul class="legal-list">
          <li><strong>Stripe, Inc.</strong> &mdash; Payment processing. Stripe receives payment information necessary to process transactions. See <a href="https://stripe.com/privacy" target="_blank" rel="noopener">Stripe's Privacy Policy</a>.</li>
          <li><strong>Sentry (Functional Software, Inc.)</strong> &mdash; Error tracking and monitoring. Sentry receives error reports with PII automatically filtered (passwords, tokens, and credit card numbers are redacted before transmission). See <a href="https://sentry.io/privacy/" target="_blank" rel="noopener">Sentry's Privacy Policy</a>.</li>
        </ul>
        <h3 class="legal-subsection-title">Legal Requirements</h3>
        <p class="legal-text">
          We may disclose your information if required to do so by law or in response to valid requests by public
          authorities (e.g., a court order or government agency). We will make reasonable efforts to notify you before
          such disclosure, unless prohibited by law.
        </p>
        <h3 class="legal-subsection-title">Business Transfers</h3>
        <p class="legal-text">
          If Wisdoverse Forge is involved in a merger, acquisition, or asset sale, your personal information may be
          transferred as part of that transaction. We will provide notice before your personal information is
          transferred and becomes subject to a different privacy policy.
        </p>
        <h3 class="legal-subsection-title">With Your Consent</h3>
        <p class="legal-text">
          We may share your information with third parties when you explicitly consent to or request such sharing.
        </p>
      </div>

      <div class="legal-section">
        <h2 class="legal-section-title">5. Your Rights</h2>
        <p class="legal-text">
          In accordance with the General Data Protection Regulation (GDPR) and other applicable data protection laws,
          you have the following rights regarding your personal data:
        </p>
        <h3 class="legal-subsection-title">Right of Access (Article 15)</h3>
        <p class="legal-text">
          You have the right to request a copy of the personal data we hold about you, along with information about how
          we process it.
        </p>
        <h3 class="legal-subsection-title">Right to Rectification (Article 16)</h3>
        <p class="legal-text">
          You have the right to request correction of any inaccurate personal data we hold about you, and to have
          incomplete data completed.
        </p>
        <h3 class="legal-subsection-title">Right to Erasure (Article 17)</h3>
        <p class="legal-text">
          You have the right to request the deletion of your personal data when it is no longer necessary for the
          purposes for which it was collected, when you withdraw consent, or when processing is unlawful.
        </p>
        <h3 class="legal-subsection-title">Right to Data Portability (Article 20)</h3>
        <p class="legal-text">
          You have the right to receive your personal data in a structured, commonly used, and machine-readable format,
          and to transmit that data to another controller.
        </p>
        <h3 class="legal-subsection-title">Right to Restrict Processing (Article 18)</h3>
        <p class="legal-text">
          You have the right to request that we restrict the processing of your personal data in certain circumstances,
          such as when you contest the accuracy of the data or object to processing.
        </p>
        <h3 class="legal-subsection-title">Right to Object (Article 21)</h3>
        <p class="legal-text">
          You have the right to object to the processing of your personal data for direct marketing purposes or when
          processing is based on legitimate interests. Upon receiving your objection, we will cease processing unless
          we demonstrate compelling legitimate grounds that override your interests.
        </p>
        <h3 class="legal-subsection-title">Right to Withdraw Consent</h3>
        <p class="legal-text">
          Where we rely on your consent for processing, you may withdraw that consent at any time without affecting the
          lawfulness of processing that occurred before your withdrawal.
        </p>
        <h3 class="legal-subsection-title">Right to Lodge a Complaint</h3>
        <p class="legal-text">
          You have the right to lodge a complaint with a supervisory authority if you believe that our processing of
          your personal data infringes applicable data protection law.
        </p>
        <p class="legal-text">
          To exercise any of these rights, please contact us at <a href="mailto:privacy@wisdoverse.com">privacy@wisdoverse.com</a>.
          We will respond to your request within 30 days.
        </p>
      </div>

      <div class="legal-section">
        <h2 class="legal-section-title">6. Data Export & Deletion</h2>
        <p class="legal-text">
          We provide self-service tools for data export and account deletion where available:
        </p>
        <h3 class="legal-subsection-title">Data Export</h3>
        <p class="legal-text">
          You can request a copy of your personal data through the account settings page or a supported export flow.
          The export includes your profile information, agent records, event history, and configuration settings in a
          file format that can be read by other tools.
        </p>
        <h3 class="legal-subsection-title">Account Deletion</h3>
        <p class="legal-text">
          You can permanently delete your account and associated data through the account settings page or a supported
          deletion flow. Upon deletion:
        </p>
        <ul class="legal-list">
          <li>Your profile information is permanently removed</li>
          <li>Active agents are stopped</li>
          <li>Event history and agent data are deleted</li>
          <li>Saved login sessions and access keys are revoked</li>
          <li>Audit logs referencing your account are anonymized (the log entries are retained for compliance but your personal identifiers are removed)</li>
          <li>Payment records are retained as required by applicable tax and financial regulations</li>
        </ul>
        <p class="legal-text">
          Account deletion is permanent and irreversible. We recommend exporting your data before requesting deletion.
          The deletion process may take up to 30 days to propagate across all backup systems.
        </p>
      </div>

      <div class="legal-section">
        <h2 class="legal-section-title">7. Cookies & Local Storage</h2>
        <p class="legal-text">
          Wisdoverse Forge uses minimal client-side storage to provide a functional and personalized experience.
        </p>
        <h3 class="legal-subsection-title">Local Storage</h3>
        <p class="legal-text">
          We use the browser's localStorage API to store your user preferences locally on your device:
        </p>
        <ul class="legal-list">
          <li>Audio volume and mute preferences</li>
          <li>Keyboard shortcut customizations</li>
          <li>Visual workspace preferences, such as saved view settings</li>
          <li>Navigation and layout preferences that make the interface easier to reuse</li>
          <li>Theme and display settings</li>
          <li>Login session data that keeps you signed in</li>
        </ul>
        <p class="legal-text">
          This data is stored only on your local device and is not transmitted to our servers except for login session
          data used to keep your account signed in. You can clear this data at any time through your browser settings.
        </p>
        <h3 class="legal-subsection-title">Cookies</h3>
        <p class="legal-text">
          We do not use tracking cookies, advertising cookies, or third-party analytics cookies. The Service may use
          essential login cookies strictly necessary for authentication and security purposes.
        </p>
      </div>

      <div class="legal-section">
        <h2 class="legal-section-title">8. Data Retention</h2>
        <p class="legal-text">
          We retain your data only for as long as necessary to fulfill the purposes described in this Privacy Policy or
          as required by law. Our specific retention periods are:
        </p>
        <ul class="legal-list">
          <li><strong>Tool events and agent data:</strong> 90 days from creation, after which events are automatically purged by our cleanup workers</li>
          <li><strong>Account information:</strong> Retained until you delete your account or request data erasure</li>
          <li><strong>Audit logs:</strong> 1 year from creation, retained for security monitoring and compliance purposes (anonymized upon account deletion)</li>
          <li><strong>Image attachments (prompt images):</strong> 7 days (automatic lifecycle policy)</li>
          <li><strong>Image attachments (general):</strong> 30 days (automatic lifecycle policy)</li>
          <li><strong>Saved login sessions:</strong> Automatically expired and cleaned up according to their configured lifetime</li>
          <li><strong>Payment records:</strong> Retained as required by applicable tax and financial regulations (typically 7 years)</li>
          <li><strong>Error tracking data (Sentry):</strong> Subject to Sentry's retention policies (typically 90 days)</li>
        </ul>
        <p class="legal-text">
          When data reaches the end of its retention period, it is permanently deleted or anonymized. Backup copies may
          persist for an additional period consistent with our backup rotation schedule (typically up to 30 days after
          deletion from production systems).
        </p>
      </div>

      <div class="legal-section">
        <h2 class="legal-section-title">9. International Transfers</h2>
        <p class="legal-text">
          Your information may be transferred to and processed in countries other than your country of residence. These
          countries may have data protection laws that differ from those in your jurisdiction.
        </p>
        <p class="legal-text">
          When we transfer personal data internationally, we ensure appropriate safeguards are in place to protect your
          data in accordance with this Privacy Policy and applicable law. These safeguards may include:
        </p>
        <ul class="legal-list">
          <li>Standard contractual clauses approved by relevant data protection authorities</li>
          <li>Data processing agreements with our service providers that include adequate data protection commitments</li>
          <li>Ensuring that recipients are located in countries with adequate levels of data protection as determined by applicable regulatory bodies</li>
        </ul>
        <p class="legal-text">
          By using the Service, you consent to the transfer of your information to our facilities and to those of third
          parties with whom we share it as described in this Privacy Policy.
        </p>
      </div>

      <div class="legal-section">
        <h2 class="legal-section-title">10. Children's Privacy</h2>
        <div class="legal-highlight">
          <p class="legal-text">
            The Service is not intended for use by individuals under the age of 13. We do not knowingly collect
            personal information from children under 13. If you are a parent or guardian and become aware that your
            child has provided us with personal information, please contact us at
            <a href="mailto:privacy@wisdoverse.com">privacy@wisdoverse.com</a>.
          </p>
        </div>
        <p class="legal-text">
          If we become aware that we have collected personal data from a child under 13 without verification of
          parental consent, we will take steps to remove that information from our servers promptly. If you believe we
          may have any information from or about a child under 13, please contact us immediately.
        </p>
      </div>

      <div class="legal-section">
        <h2 class="legal-section-title">11. Changes to This Policy</h2>
        <p class="legal-text">
          We may update this Privacy Policy from time to time to reflect changes in our practices, technology, legal
          requirements, or other factors. When we make changes, we will:
        </p>
        <ul class="legal-list">
          <li>Update the "Effective Date" at the top of this policy</li>
          <li>Provide prominent notice within the Service for material changes</li>
          <li>Send an email notification to the address associated with your account for significant changes that affect your rights</li>
        </ul>
        <p class="legal-text">
          We encourage you to review this Privacy Policy periodically to stay informed about how we protect your
          information. Your continued use of the Service after any changes to this Privacy Policy constitutes your
          acceptance of the updated policy.
        </p>
      </div>

      <div class="legal-section">
        <h2 class="legal-section-title">12. Contact</h2>
        <div class="legal-contact">
          <p class="legal-text">
            If you have any questions, concerns, or requests regarding this Privacy Policy or our data practices,
            please contact us:
          </p>
          <p class="legal-text">
            <strong>Wisdoverse Forge</strong><br>
            Privacy Inquiries: <a href="mailto:privacy@wisdoverse.com">privacy@wisdoverse.com</a><br>
            Data Protection Officer: <a href="mailto:dpo@wisdoverse.com">dpo@wisdoverse.com</a>
          </p>
          <p class="legal-text">
            We are committed to resolving complaints about your privacy and our collection or use of your personal
            information. We will respond to all privacy-related inquiries within 30 days.
          </p>
        </div>
      </div>
    `
  }
}
