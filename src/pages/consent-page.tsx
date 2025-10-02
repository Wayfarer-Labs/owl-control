import React, { useState, useRef, useEffect } from "react";
import { Button } from "@/components/ui/button";
import { AuthService } from "@/services/auth-service";
import { ElectronService } from "@/services/electron-service";

interface ConsentPageProps {
  apiKey: string;
  onConsent: () => void;
  onCancel: () => void;
}

export function ConsentPage({ apiKey, onConsent, onCancel }: ConsentPageProps) {
  const [hasReadConsent, setHasReadConsent] = useState(false);
  const consentRef = useRef<HTMLDivElement>(null);

  // Function to check if user has scrolled to bottom of consent
  const checkScrollPosition = () => {
    if (consentRef.current) {
      const { scrollTop, scrollHeight, clientHeight } = consentRef.current;
      // Make the threshold larger to ensure it's easier to trigger
      if (scrollTop + clientHeight >= scrollHeight - 50) {
        setHasReadConsent(true);
      }
    }
  };

  // Also check scroll on mount and add click handler as fallback
  useEffect(() => {
    // Check initial scroll position
    checkScrollPosition();

    // Add click handler to content as fallback to enable button
    const handleContentClick = () => {
      setTimeout(() => {
        setHasReadConsent(true);
      }, 2000); // Enable after 2 seconds of user interaction
    };

    const currentRef = consentRef.current;
    if (currentRef) {
      currentRef.addEventListener("click", handleContentClick);
    }

    return () => {
      if (currentRef) {
        currentRef.removeEventListener("click", handleContentClick);
      }
    };
  }, []);

  const handleConsent = async () => {
    try {
      // Store API key and mark as consented
      const authService = AuthService.getInstance();
      await authService.validateApiKey(apiKey);
      await authService.setConsent(true);

      // Notify Electron to update the tray menu and close the window
      try {
        await ElectronService.authenticationCompleted();
      } catch (error) {
        console.error(
          "Error notifying Electron about authentication completion:",
          error,
        );
      }

      onConsent();
    } catch (error) {
      console.error("Error saving consent:", error);
    }
  };

  const handleCancel = async () => {
    try {
      // Ensure user is not marked as consented
      const authService = AuthService.getInstance();
      await authService.setConsent(false);
      onCancel();
    } catch (error) {
      console.error("Error canceling consent:", error);
      onCancel();
    }
  };

  return (
    <div
      className="consent-page flex flex-col h-screen"
      style={{ backgroundColor: "#0c0c0f" }}
    >
      {/* Draggable header area */}
      <div
        className="h-8"
        style={{ WebkitAppRegion: "drag", "-webkit-app-region": "drag" } as any}
      ></div>

      <div
        className="consent-page-content flex-1 flex items-center justify-center p-6"
        style={{ backgroundColor: "#0c0c0f" }}
      >
        <div className="w-full max-w-[700px] space-y-6">
          <div className="text-center">
            <h1 className="text-3xl font-bold text-white mb-2">
              Informed Consent & Terms of Service
            </h1>
            <p className="text-[#828691]">
              Please read the following information carefully
            </p>
          </div>

          <div className="bg-[#1a1d23] rounded-lg border border-[#2a2d35] overflow-hidden">
            <div
              ref={consentRef}
              className="h-[400px] overflow-y-auto p-6"
              onScroll={checkScrollPosition}
            >
              <div className="space-y-4 text-[#c5c7cb]">
                <div>
                  <h2 className="text-lg font-bold text-white mb-2">
                    Research Study: Wayfarer Labs Interactive Game Data Pilot
                  </h2>
                  <p className="text-sm">
                    Principal Investigators: Louis Castricato, Shahbuland
                    Matiana
                    <br />
                    Contact: louis@wayfarerlabs.ai, shahbuland@wayfarerlabs.ai
                  </p>
                </div>

                <div>
                  <h3 className="text-md font-bold text-white mb-1">
                    Purpose of Study
                  </h3>
                  <p className="text-sm">
                    You are invited to participate in a research study aiming to
                    collect combined game and control data for the purposes of
                    training world models and subsequently AI agents. The
                    curated dataset will be open sourced and made publicly
                    available for research purposes. This software will record
                    your game and inputs to potentially contribute to this
                    dataset. There is no minimum required participation.
                  </p>
                </div>

                <div>
                  <h3 className="text-md font-bold text-white mb-1">
                    Procedures
                  </h3>
                  <p className="text-sm">
                    If you agree to participate our software will:
                    <br />
                    - Record your screen during gameplay sessions
                    <br />
                    - Log keyboard and mouse inputs
                    <br />- Store this data for research purposes
                  </p>
                </div>

                <div>
                  <h3 className="text-md font-bold text-white mb-1">
                    Data Collection and Privacy
                  </h3>
                  <p className="text-sm">
                    - The recording software is limited to only capturing full
                    screen applications.
                    <br />
                    - The recording software will stop recording if no activity
                    is detected for a sufficient period of time.
                    <br />
                    - Further processing and cleaning will be done before any
                    open source release. During this process, the data will be
                    stored securely and anonymized.
                    <br />
                    - Upon full open source release, there will be no
                    identifying information in the dataset.
                    <br />
                    - The software cannot record microphone audio
                    <br />
                    - The software records game audio only, not all desktop
                    audio
                    <br />
                    - Data is stored locally and only uploaded when you manually
                    press the upload button
                    <br />- The software avoids recording overlays.
                  </p>
                </div>

                <div>
                  <h3 className="text-md font-bold text-white mb-1">
                    Potential Risks
                  </h3>
                  <p className="text-sm">
                    - Using OWL Control in multiplayer games may result in
                    account bans, as anti-cheat systems may flag it as
                    suspicious software
                    <br />- We strongly recommend using OWL Control only in
                    single-player games to avoid potential issues
                  </p>
                </div>

                <div>
                  <h3 className="text-md font-bold text-white mb-1">
                    Voluntary Participation
                  </h3>
                  <p className="text-sm">
                    Your participation is entirely voluntary. You may:
                    <br />
                    - Choose not to participate
                    <br />
                    - Stop recording at any time
                    <br />
                    - Request deletion of your recorded data
                    <br />- Withdraw from the study without penalty
                  </p>
                </div>

                <div>
                  <h3 className="text-md font-bold text-white mb-1">
                    Compensation
                  </h3>
                  <p className="text-sm">
                    - There is no compensation for this study
                  </p>
                </div>

                <div>
                  <h3 className="text-md font-bold text-white mb-1">
                    Questions or Concerns
                  </h3>
                  <p className="text-sm">
                    For questions about this research, contact
                    shahbuland@wayfarerlabs.ai
                  </p>
                </div>

                <div>
                  <h3 className="text-md font-bold text-white mb-1">
                    Content Policy & Legal Terms
                  </h3>
                  <p className="text-sm">
                    <strong>Important:</strong> You agree not to upload any
                    content that is:
                    <br />
                    - Illegal in your jurisdiction or country of origin
                    <br />
                    - Malicious, harmful, or inappropriate
                    <br />
                    - In violation of any applicable laws or regulations
                    <br />
                    <br />
                    If you upload content that violates these terms, we will
                    take necessary and proportional actions, which may include:
                    <br />
                    - Removal of your content
                    <br />
                    - Suspension or termination of your access
                    <br />
                    - Reporting to appropriate authorities
                    <br />- Other legal remedies as required by law
                  </p>
                </div>

                <div>
                  <h3 className="text-md font-bold text-white mb-1">Consent</h3>
                  <p className="text-sm">
                    By clicking "Accept" below you confirm that:
                    <br />
                    - You have read and understood the above information
                    <br />
                    - You are 18 years or older
                    <br />
                    - You voluntarily agree to participate
                    <br />
                    - You understand you can withdraw at any time
                    <br />- You agree to comply with the content policy and
                    legal terms
                  </p>
                </div>

                <div className="h-32 flex items-center justify-center border-t border-[#2a2d35] mt-6">
                  <p className="text-sm text-[#828691]">End of Document</p>
                </div>
              </div>
            </div>
          </div>

          {!hasReadConsent && (
            <p className="text-sm text-[#ffc107] text-center">
              Please scroll to the bottom of the document to enable the consent
              button
            </p>
          )}

          <div className="flex justify-between gap-4">
            <button
              disabled={!hasReadConsent}
              onClick={handleCancel}
              className="flex-1 h-12 font-medium transition-colors border border-[#2a2d35] rounded-md"
              style={{
                backgroundColor: !hasReadConsent
                  ? "transparent"
                  : "transparent",
                color: !hasReadConsent ? "#4a4e58" : "white",
                borderColor: !hasReadConsent ? "#1a1d23" : "#2a2d35",
                cursor: !hasReadConsent ? "not-allowed" : "pointer",
                opacity: !hasReadConsent ? 0.5 : 1,
              }}
            >
              I do not consent
            </button>

            <button
              disabled={!hasReadConsent}
              onClick={handleConsent}
              className="flex-1 h-12 font-medium transition-colors rounded-md"
              style={{
                backgroundColor: !hasReadConsent ? "#1a1d23" : "#42e2f5",
                color: !hasReadConsent ? "#4a4e58" : "#0c0c0f",
                cursor: !hasReadConsent ? "not-allowed" : "pointer",
                opacity: !hasReadConsent ? 0.5 : 1,
              }}
            >
              I agree to the informed consent & terms of service
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
