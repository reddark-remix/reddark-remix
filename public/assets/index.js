var audioSystem = {
    playAudio: false,
    play: function (file) {
        var audio = new Audio('/assets/audio/' + file + ".mp3");
        if (this.playAudio == true)
            audio.play();
    }
}

var block = ["r/gtafk","r/bi_irl", "r/suddenlybi", "r/ennnnnnnnnnnnbbbbbby", "r/feemagers", "r/BrexitAteMyFace", "r/emoney", "r/Inzaghi"];

document.getElementById("enable_sounds").addEventListener("click", function () {
    if (!audioSystem.playAudio) {
        this.innerHTML = "Disable sound alerts"
        audioSystem.playAudio = true;
        audioSystem.play("privated");
        newStatusUpdate("Enabled audio alerts.");
    } else {
        audioSystem.playAudio = false;
        newStatusUpdate("Disabled audio alerts.");
        this.innerHTML = "Enable sound alerts"
    }
})

function doScroll(el) {
    const elementRect = el.getBoundingClientRect();
    const absoluteElementTop = elementRect.top + window.pageYOffset;
    const middle = absoluteElementTop - (window.innerHeight / 2);
    window.scrollTo(0, middle);
}

function newStatusUpdate(text, callback = null, _classes = []) {
    var item = Object.assign(document.createElement("div"), {"className": "status-update"});
    item.innerHTML = text;
    document.getElementById("statusupdates").appendChild(item);
    setTimeout(() => {
        item.remove();
    }, 5000);

    item.addEventListener("click", function () {
        item.remove();
        if (callback != null) {
            callback();
        }
    })
    for (var _class of _classes) {
        item.classList.add(_class);
    }
}

function mapState(state) {
    switch (state) {
        case "PUBLIC":
            return "public";
        case "PRIVATE":
            return "private";
        case "RESTRICTED":
            return "restricted";
        default:
            return "unknown";
    }
}

function sectionBaseName(section) {
    var section_basename = section.replace(" ", "").replace(":", "").replace("+", "").replace(" ", "").replace("\r", "").replace("\n", "");
    return section_basename;
}

var totalDarkSubs = parseInt(document.getElementById("lc-count").innerText);
var totalPrivateSubs = 0;
var totalUnlistedSubs = 0;
var totalSubs = parseInt(document.getElementById("lc-total").innerText);

function updateStatusText() {
    document.getElementById("st-total").innerText = totalSubs;
    document.getElementById("st-dark").innerText = totalDarkSubs;
    od.update(totalDarkSubs);
    document.getElementById("lc-total").innerText = totalSubs;

    var percentage = ((totalDarkSubs / totalSubs) * 100).toFixed(2);
    var percentage = ((totalDarkSubs / totalSubs) * 100).toFixed(2);
    od_percentage.update(percentage);
    od_togo.update(totalSubs - totalDarkSubs);
    document.getElementById("progress-bar").style = `width: ${percentage}%`;
}


function handleStateUpdate(message) {
    totalSubs = message["subreddits"].length;
    totalDarkSubs = 0;
    for (subreddit of message["subreddits"]) {
        switch (subreddit["state"]) {
            case "PRIVATE":
            case "RESTRICTED":
                totalDarkSubs += 1;
                break;
            default:
                break;
        }
    }

    var newHtml = "";
    for (section of message["sections"]) {
        newHtml += `<h1>${section}</h1>\n`;
        newHtml += `<div class="section-grid">\n`;
        for (subreddit of message["subreddits"]) {
            if (subreddit["section"] === section) {
                var s = mapState(subreddit["state"]);
                newHtml += `<div class="subreddit subreddit-${s}" id="${subreddit["name"]}">\n`;
                newHtml += `<a href="https://old.reddit.com/${subreddit.name}" target="_blank" rel="noopener noreferrer">${subreddit.name}</a>\n`;
                newHtml += `<p>${s}</p>\n`;
                newHtml += `</div>\n`;
            }
        }
        newHtml += `</div>\n`;
    }

    document.getElementById("list").innerHTML = newHtml;
    updateStatusText();
}

function handleDeltaUpdate(message) {
    if (message["name"] in block || block.includes(message["name"])) {
        return;
    }

    var text = `<strong>${message["name"]}</strong> has gone ${mapState(message["state"])}! (${message["section"]})`;

    // Send out status update for people not in large counter mode.
    newStatusUpdate(text, function () {
        doScroll(document.getElementById(message["name"]));
    }, ["n" + sectionBaseName(message["section"])]);

    // Update state in current view if present.
    if (document.getElementById(message["name"]) != null) {
        document.getElementById(message["name"]).querySelector("p").innerHTML = mapState(message["state"]);
        for (i of ["private", "public", "restricted", "unknown"]) {
            document.getElementById(message["name"]).classList.remove(`subreddit-${i}`)
        }
        document.getElementById(message["name"]).classList.add(`subreddit-${mapState(message["state"])}`)
    }

    switch (message["state"]) {
        case "PRIVATE":
        case "RESTRICTED":
            audioSystem.play("privated");
            switch (subreddit["previous_state"]) {
                case "PUBLIC":
                case "UNKNOWN":
                    totalDarkSubs += 1;
                    break;
                default:
                    break;
            }
            break;
        default:
            audioSystem.play("public");
            totalDarkSubs -= 1;
            break
    }
    updateStatusText();


    var history_item = Object.assign(document.createElement("div"), {className: "history-item n" + sectionBaseName(message["section"])});
    var t = new Date().toISOString().replace("T", " ").replace(/\..+/, '');
    history_item.innerHTML = `<h1><strong>${message["name"]}</strong> has gone ${mapState(message["state"])}! (${message["section"]})</h1><h3>${t}</h3>`;

    switch (message["state"]) {
        case "PUBLIC":
        case "UNKNOWN":
            history_item.classList.add("history-item-online")
            break;
        default:
            break;
    }
    document.getElementById("counter-history").prepend(history_item);
    document.getElementById("counter-history").scrollTo({top: 0, behavior: 'smooth'});
}

var eventSource = newEventSource();

function newEventSource() {
    var eventSource = new EventSource('sse');

    eventSource.onopen = function (event) {
        console.log("Server connection open!");
    }

    eventSource.onerror = function (event) {
        console.log("Error with event source. Reconnect in 3 seconds...");
        eventSource.close();
        setTimeout(() => {
            eventSource = newEventSource();
        }, 3000);
    }

    eventSource.onmessage = function (event) {
        console.log('Message from server ', event.data);
        const message = JSON.parse(event.data);
        switch (message.type) {
            case "CurrentStateUpdate":
                handleStateUpdate(message["content"]);
                break;
            case "Delta":
                handleDeltaUpdate(message["content"]);
                break;
            case "Reload":
                window.location.reload();
                break;
            default:
                break;
        }
    }
    // Maintain filters by search params after event triggered state update
    filterBySearch();
    return eventSource;
}

function hidePublicSubreddits() {
    document.getElementById("list").classList.remove("hide-private");
    document.getElementById("list").classList.toggle("hide-public");
    document.getElementById("hide-public").classList.toggle("toggle-enabled");
    document.getElementById("hide-private").classList.remove("toggle-enabled");
    // Provide same behavior as search when section is all hidden
    handleHeaders();
}

function hidePrivateSubreddits() {
    document.getElementById("list").classList.remove("hide-public");
    document.getElementById("list").classList.toggle("hide-private");
    document.getElementById("hide-private").classList.toggle("toggle-enabled");
    document.getElementById("hide-public").classList.remove("toggle-enabled");
    // Provide same behavior as search when section is all hidden
    handleHeaders();
}


function toggleLargeCounter() {
    document.getElementById("large-counter").classList.toggle("large-counter-hidden");
    document.body.classList.toggle("noscroll");
}

od_percentage = new Odometer({
    el: document.getElementById("percentage"),
    value: document.getElementById("percentage").innerText,
    format: '(,ddd).dd',
    theme: 'default'
});
od_togo = new Odometer({
    el: document.getElementById("togo"),
    value: document.getElementById("togo").innerText,
    format: '',
    theme: 'default'
});
od = new Odometer({
    el: document.getElementById("lc-count"),
    value: document.getElementById("lc-count").innerText,
    format: '',
    theme: 'default'
});
