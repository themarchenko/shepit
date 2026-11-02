import Foundation

public enum CalibrationOffer {
    /// The wizard opens by itself once, for someone whose speaker notes are still unset, when nothing else is
    /// asking for attention and an agent can answer. The legacy default sentence counts as unset.
    public static func shouldAutoOpen(offered: Bool, onboardingCompleted: Bool, tourOpen: Bool, doctorOpened: Bool,
                                      requiredProblem: Bool, speakerNotes: String, agentAvailable: Bool) -> Bool {
        !offered && onboardingCompleted && !tourOpen && !doctorOpened && !requiredProblem
            && SpeakerNotes.isUnset(speakerNotes) && agentAvailable
    }
}
