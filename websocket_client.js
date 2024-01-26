// this will be embeded in the response to client to reload the page
// when there is a change in elm code.

const socketOnOpen = (_event) => {
  this.send("ready");
}

const socketOnMsg = (event) => {
  const msg = event.data;
  if (msg == "reload") {
    window.location.reload();
  }
}

window.onload = (_event) => {
  if (!window.WebSocket) {
    console.log("This browser doesn't support websockets, hot reloading is not support");
    return;
  }

  const webSocketUrl = window.location.origin.replace(/(^http(s?):\/\/)(.*)/, 'ws$2://$3')
  const webSocket = new WebSocket(webSocketUrl);

  webSocket.onopen = socketOnOpen;
  webSocket.onmessage = socketOnMsg;

  webSocket.onclose = (_event) => {
    console.log("connection closed");
  }

  webSocket.onerror = (event) => {
    console.log("encounterd an error: ", event);
  }
}
