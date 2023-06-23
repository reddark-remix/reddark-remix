// by https://github.com/Krafalski

const searchBar = document.querySelector("input[type='search'");

// Show/hide subreddits that match search inputs
searchBar.addEventListener("input", filterBySearch);

function filterBySearch(e) {
  // Stop unexpected scroll on mobile (hopefully) when user inputs
  if (e) {
    e.target.focus({ preventScroll: true });
  }

  // Always compare lowercase letters
  // No matter what the user inputs
  // Since page refreshes, don't use event.target, since this function
  // can be triggered by both the input event and eventSource.onmessage  (index.js)
  const input = searchBar.value.toLowerCase();

  // Reset the filters before filterning anew
  resetFilterBySearch();

  // Select the divs with the subreddit names
  const subredditDivs = document.querySelectorAll(".subreddit");

  // Check sub names against user input, add an inline style of none for those that do not match
  // This must be done inline to override any selections from the show/hide private/restricted/public/unkown tools
  for (let div of subredditDivs) {
    const subName = div.querySelector("a").textContent.toLowerCase();
    if (!subName.includes(input)) {
      div.style.display = "none";
    } else {
      // Empty string removes inline style and allows other applied styles to have control
      div.style.display = "";
    }
  }
  // Remove headers that are empty
  handleHeaders();
  // Add an error message if there are no results
  handleNoResults();
}

// Set display to none on the header headers that have empty sections
function handleHeaders() {
  // Only select h1 elements inside the main component
  const headers = document.querySelectorAll("main h1");
  // Reset header displays
  for (let header of headers) {
    header.style.display = "";
  }
  // The subreddits are contained in the next element after the h1
  // Check each one, if all have a display of none, hide the header
  for (let header of headers) {
    const sectionGrid = header.nextElementSibling;
    const sectionGridSubDivs = sectionGrid.querySelectorAll("div");
    let allFiltered = true;

    for (let div of sectionGridSubDivs) {
      // Divs have style from CSS or inline, use this to get that style
      // Whichever it is
      const style = window.getComputedStyle(div);
      const divDisplay = style.getPropertyValue("display");

      if (divDisplay !== "none") {
        allFiltered = false;
        break;
      }
    }
    // If all the subreddits are filtered out, hide the heading
    if (allFiltered) {
      header.style.display = "none";
    }
  }
}

function handleNoResults() {
  // Select the main element so an error message can be displayed if no results are found
  const main = document.querySelector("main");
  // The message will be determined if any h1s have a display other than none
  const mainHeaders = document.querySelectorAll("main h1");
  let createNoResultsMessage = true;
  // check style display properties
  for (let header of mainHeaders) {
    if (header.style.display !== "none") {
      createNoResultsMessage = false;
    }
  }
  // Create and append error message
  if (createNoResultsMessage) {
    const noResults = document.createElement("h2");
    noResults.id = "no-results";
    noResults.innerText = "Your query returned no results";
    main.append(noResults);
  }
}

// Reset all display settings before applying new ones
function resetFilterBySearch() {
  // Select the divs with the subreddit names
  const subredditDivs = document.querySelectorAll(".subreddit");
  // Select header for subreddits
  const mainHeaders = document.querySelectorAll("main h1");

  for (let div of subredditDivs) {
    div.style.display = "";
  }

  // remove the error message, if present
  const noResults = document.querySelector("#no-results");
  if (noResults) {
    noResults.remove();
  }
}
