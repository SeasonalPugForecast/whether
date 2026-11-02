import SwiftUI

extension EnvironmentValues {
    /// The leading temperatures are feels-like ones (the core's
    /// `feelsLikeLeads`): views say so, and show the air temperature beside them.
    @Entry var feelsLikeLeads = false
}
