//! Defense AI trivia database for splash carousel and /trivia command.
//!
//! REQ-TUI-112: 50+ historical facts about AI in defense, intelligence,
//! and national security. Displayed during model loading splash and
//! available on demand via /trivia.

/// Defense AI trivia facts. Each entry is a self-contained historical
/// fact suitable for display in a single TUI line or short paragraph.
pub const TRIVIA_FACTS: &[&str] = &[
    // Early AI and defense origins
    "The term 'artificial intelligence' was coined at the 1956 Dartmouth \
     workshop, funded in part by the Office of Naval Research.",
    "DARPA (then ARPA) funded the first AI research labs at MIT, Stanford, \
     and Carnegie Mellon in the early 1960s.",
    "The ARPANET, DARPA's predecessor to the internet, went live in 1969 \
     connecting four university research nodes.",
    "Shakey the Robot (1966-1972), built at SRI with DARPA funding, was \
     the first mobile robot to reason about its own actions.",
    "The DENDRAL project (1965), partially funded by NASA, was one of the \
     first expert systems -- it identified chemical compounds from mass \
     spectrometry data.",
    // Cold War era
    "SAGE (Semi-Automatic Ground Environment), operational 1958-1983, was \
     the first large-scale computerized air defense system, using AI-like \
     pattern matching to track Soviet bombers.",
    "The Navy's DART (Dynamic Analysis and Replanning Tool) managed logistics \
     during Desert Storm in 1991 -- DARPA said it repaid the entire 30-year \
     AI investment.",
    "DARPA's Strategic Computing Initiative (1983-1993) invested $1 billion \
     in AI for autonomous vehicles, battle management, and speech recognition.",
    "The CIA's AQUAINT program (2002-2007) advanced question-answering AI \
     to help analysts find answers in massive text corpora.",
    "ELIZA (1966) at MIT, while not defense-funded, inspired military interest \
     in natural language processing for intelligence analysis.",
    // Autonomous systems
    "DARPA's Grand Challenge (2004) offered $1M for an autonomous vehicle to \
     cross 142 miles of Mojave Desert. No vehicle finished. In 2005, five did.",
    "The MQ-1 Predator drone, first deployed in 1995, evolved from a \
     reconnaissance platform to carry Hellfire missiles by 2001, sparking \
     decades of debate on autonomous weapons.",
    "The X-47B became the first unmanned aircraft to launch from and land on \
     an aircraft carrier in 2013, demonstrating autonomous carrier operations.",
    "The Aegis Combat System, namesake of this tool, uses phased-array radar \
     and computer-directed fire control to defend naval battle groups. First \
     deployed in 1983.",
    "DARPA's ACTUV (Anti-Submarine Warfare Continuous Trail Unmanned Vessel) \
     Sea Hunter can autonomously track submarines for months at a time.",
    "The Phalanx CIWS (Close-In Weapon System) has operated in autonomous \
     mode since 1980, automatically detecting and engaging incoming missiles.",
    "Israel's Iron Dome, operational since 2011, uses AI to predict incoming \
     rocket trajectories and intercept only those threatening populated areas.",
    "The U.S. Army's Project Maven (2017) applied machine learning to drone \
     footage analysis, becoming one of the most debated military AI programs.",
    "DARPA's AlphaDogfight Trials (2020) pitted an AI agent against a human \
     F-16 pilot in simulated combat. The AI won 5-0.",
    "The UK's Taranis stealth UCAV demonstrator completed fully autonomous \
     flight tests in 2013, including simulated weapons release.",
    // Intelligence and signals
    "The NSA has used machine learning for signals intelligence (SIGINT) \
     classification since the early 2000s, processing billions of intercepts.",
    "Palantir Technologies, founded in 2003 with CIA seed funding, built \
     data fusion platforms used across the intelligence community.",
    "The NRO (National Reconnaissance Office) uses computer vision AI to \
     analyze satellite imagery, a capability that evolved from Cold War \
     photo interpretation.",
    "DARPA's Total Information Awareness (TIA) program (2002-2003) aimed to \
     detect terrorist planning through pattern analysis of transaction data. \
     Congress defunded it over privacy concerns.",
    "The Intelligence Advanced Research Projects Activity (IARPA), modeled \
     after DARPA, was created in 2006 to fund high-risk intelligence research \
     including AI forecasting.",
    "Babylon, a DARPA-funded speech translation system, was deployed in Iraq \
     to help soldiers communicate with Arabic speakers in real time.",
    "The Air Force's Distributed Common Ground System (DCGS) processes \
     over 1,500 hours of full-motion video daily using AI-assisted analysis.",
    "ECHELON, the Five Eyes signals intelligence network, reportedly began \
     using keyword spotting AI for automated intercept filtering in the 1990s.",
    // Cyber and information warfare
    "Stuxnet (discovered 2010), widely attributed to U.S. and Israeli \
     intelligence, was one of the first cyberweapons to cause physical \
     damage -- it destroyed Iranian centrifuges.",
    "DARPA's Cyber Grand Challenge (2016) was the first all-machine hacking \
     tournament, where AI systems found and patched software vulnerabilities \
     in real time.",
    "The U.S. Cyber Command (USCYBERCOM), established 2009, increasingly \
     relies on AI for network defense, threat hunting, and offensive operations.",
    "China's 2017 New Generation AI Development Plan explicitly targets \
     military-civil AI fusion, aiming for AI dominance by 2030.",
    "Russia's military AI strategy centers on the Marker autonomous combat \
     robot platform and the Poseidon autonomous nuclear torpedo.",
    // Modern era
    "The DoD's Joint Artificial Intelligence Center (JAIC), established \
     2018, was the first centralized U.S. military AI organization. It \
     merged into the Chief Digital and AI Office (CDAO) in 2022.",
    "The National Security Commission on AI (NSCAI), chaired by Eric Schmidt, \
     published its final report in 2021 warning that the U.S. was not \
     prepared for AI-era national security threats.",
    "The Pentagon's Replicator initiative (2023) aims to field thousands \
     of autonomous drones to counter China's military mass.",
    "Ukraine's use of AI-enabled drones, satellite imagery analysis, and \
     facial recognition in its defense against Russia (2022-present) is \
     the largest real-world test of military AI to date.",
    "GPT-4 was evaluated by the Pentagon for military planning tasks in 2023, \
     marking the first known assessment of large language models for \
     operational military use.",
    "The U.S. Air Force's ABMS (Advanced Battle Management System) uses AI \
     to connect sensors and shooters across all military domains.",
    "NATO established its first AI strategy in 2021 and the Defence \
     Innovation Accelerator for the North Atlantic (DIANA) in 2022.",
    "The DoD adopted its Responsible AI Principles in February 2020: \
     responsible, equitable, traceable, reliable, and governable.",
    "DARPA's Lifelong Learning Machines (L2M) program aims to create AI \
     that learns continuously in the field, unlike static trained models.",
    // Historical computing and cryptography
    "Bletchley Park's Colossus (1943-1945), built to break Lorenz cipher \
     traffic, is considered the first programmable electronic digital \
     computer -- created for defense intelligence.",
    "Alan Turing's work breaking Enigma at Bletchley Park saved an estimated \
     14 million lives and shortened WWII by more than two years.",
    "The ENIAC (1945), the first general-purpose electronic computer, was \
     built to compute artillery firing tables for the U.S. Army.",
    "GPS, now essential for autonomous navigation, was developed by the DoD \
     starting in 1973 and declared fully operational in 1995.",
    "The Internet Protocol (TCP/IP) was developed under DARPA contract by \
     Vint Cerf and Bob Kahn in 1974, originally for military networking.",
    // Ethics and policy
    "The Campaign to Stop Killer Robots, launched in 2013, advocates for a \
     preemptive ban on fully autonomous weapons under international law.",
    "DoD Directive 3000.09 (2012, updated 2023) requires human oversight \
     for autonomous weapons systems that select and engage targets.",
    "The Wassenaar Arrangement restricts export of dual-use surveillance \
     and intrusion software, including some AI-powered cyber tools.",
    "In 2018, thousands of Google employees protested Project Maven, leading \
     Google to establish AI ethics principles and not renew the contract.",
    "The Political Declaration on Responsible Military Use of AI (2023), \
     endorsed by 50+ nations, established the first international norms \
     for military AI.",
    // Space and nuclear
    "The Space Force's Kobayashi Maru program uses AI for space domain \
     awareness, tracking thousands of objects in orbit.",
    "DARPA's Space-BACN (Space-Based Adaptive Communications Node) aims \
     to connect military satellite constellations using AI-driven routing.",
    "The NC3 (Nuclear Command, Control, and Communications) system has \
     strict policies against AI in nuclear launch decisions, maintained \
     since the 1980s.",
    "Project 985/211 (China) and the Assured AI program (UK) are parallel \
     national efforts to develop trustworthy military AI systems.",
    "The Air Force Research Laboratory's Skyborg program pairs autonomous \
     wingman drones with manned fighter aircraft using onboard AI.",
];

/// Return a trivia fact selected by index (wrapping). Useful for
/// splash carousel where the index increments on each tick.
pub fn fact_by_index(index: usize) -> &'static str {
    TRIVIA_FACTS[index % TRIVIA_FACTS.len()]
}

/// Return a pseudorandom trivia fact based on a simple seed.
/// Uses a lightweight hash to avoid pulling in rand as a dependency.
pub fn random_fact(seed: u64) -> &'static str {
    let index = (seed.wrapping_mul(6364136223846793005).wrapping_add(1)) as usize;
    TRIVIA_FACTS[index % TRIVIA_FACTS.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-TUI-112
    #[test]
    fn trivia_database_has_at_least_50_facts() {
        assert!(
            TRIVIA_FACTS.len() >= 50,
            "Expected at least 50 trivia facts, got {}",
            TRIVIA_FACTS.len()
        );
    }

    // rtmx:req REQ-TUI-112
    #[test]
    fn trivia_facts_are_nonempty() {
        for (i, fact) in TRIVIA_FACTS.iter().enumerate() {
            assert!(!fact.trim().is_empty(), "Trivia fact at index {i} is empty");
        }
    }

    // rtmx:req REQ-TUI-112
    #[test]
    fn trivia_facts_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for fact in TRIVIA_FACTS {
            assert!(seen.insert(*fact), "Duplicate trivia fact: {fact}");
        }
    }

    // rtmx:req REQ-TUI-112
    #[test]
    fn fact_by_index_wraps() {
        let len = TRIVIA_FACTS.len();
        assert_eq!(fact_by_index(0), fact_by_index(len));
        assert_eq!(fact_by_index(1), fact_by_index(len + 1));
    }

    // rtmx:req REQ-TUI-113
    #[test]
    fn random_fact_returns_valid_fact() {
        let fact = random_fact(42);
        assert!(
            TRIVIA_FACTS.contains(&fact),
            "random_fact returned unknown fact: {fact}"
        );
    }

    // rtmx:req REQ-TUI-113
    #[test]
    fn random_fact_varies_with_seed() {
        // Different seeds should (usually) produce different facts.
        // With 55 facts and distinct seeds, collision is unlikely.
        let facts: std::collections::HashSet<&str> = (0u64..20).map(random_fact).collect();
        assert!(
            facts.len() > 1,
            "random_fact should produce varied results across seeds"
        );
    }
}
