async function loadRegistryData() {
    try {
        const response = await fetch('http://127.0.0.1:8000/api/v1/registry');
        const data = await response.json();
        return data;
    } catch (error) {
        console.error("Failed to load registry data:", error);
        return null;
    }
}

let feedEvents = [];

async function initLeaderboard() {
    const registry = await loadRegistryData();
    if (!registry) return;

    // Populate Leaderboard
    const container = document.getElementById('leaderboard');
    container.innerHTML = registry.leaderboard.map(agent => `
        <div class="agent-item">
            <div class="agent-info">
                <span class="agent-name">${agent.name}</span>
                <span class="agent-id">${agent.id} | Hash: ${agent.last_hash.substring(0, 10)}...</span>
            </div>
            <span class="score">${agent.score}</span>
        </div>
    `).join('');

    // Setup Feed Events
    if (registry.traces && registry.traces.length > 0) {
        feedEvents = registry.traces.map(t => ({
            msg: `Agent ${t.agent_id} verified. Hash: ${t.trace_hash.substring(0, 12)}...`,
            status: t.state === 'COMPLETED' ? 'success' : 'error'
        }));
    } else {
        feedEvents = registry.feed || [];
    }
    
    // Initial batch of feed items
    for(let i=0; i<Math.min(5, feedEvents.length); i++) {
        addFeedItem();
    }
}

function addFeedItem() {
    if (feedEvents.length === 0) return;
    
    const feed = document.getElementById('verification-feed');
    const event = feedEvents[Math.floor(Math.random() * feedEvents.length)];
    const timestamp = new Date().toLocaleTimeString('en-GB', { hour12: false });
    
    const item = document.createElement('div');
    item.className = `feed-item ${event.status}`;
    item.innerHTML = `
        <span class="timestamp">[${timestamp}]</span>
        <span class="event-msg">${event.msg}</span>
    `;
    
    feed.prepend(item);
    
    if (feed.children.length > 50) {
        feed.lastElementChild.remove();
    }
}

// Kickstart
document.addEventListener('DOMContentLoaded', () => {
    initLeaderboard();
    
    // Simulate live traffic using the loaded real hashes
    setInterval(addFeedItem, 2500);
});
