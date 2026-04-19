# Requirements
ID: ARCH-REQ  
Status: PRELIMINARY  
Depends on: STD-DOC

## Requirements
i want the mlfilteredbrowser to run ml models at native performance. for example, i ran an ml model in browser extension and in localhost server, and localhost server was significantly faster. i want the "browser" (not full browser) to render static html and run various user chosen ml models on it. i want the user to choose whether to optimize for latency critical or throughput critical or maybe other settings for each ml model, but maybe the browser can determine the better option and override user's selection on specific ml model and notify user about it. since some websites are dynamic, i want separate external loader to inject html/css/js into browser, but then I want to prevent post/put/delete requests to prevent complexity in browser. i want to try to test browser via get only mode, and i want to check how other dynamic websites are working. i do not know how to solve the problem of dynamic websites. on the one hand, i want the browser to be simple for me to implement and ml models being able to run fast without changing dynamic content, but on the other hand, i still want dynamic wwebsites to be useful. there is also the issue of security problems. i do not know exactly what other requirements should look like.
