# RMC Backend

For a general overview of how the site works, take a look at the [Readme for the entire site](../README.md). This is a more technical overview of the backend.

Note: This repo has been stripped of any sensitive user data.

## Overview
All web requests go through the NGINX webserver. Static content (at set locations) is served by NGINX. Requests for dynamic content (ie. requests with specific queries or to the API) are passed to the rust webserver, a proxy server.

### The Rust Webserver
This is a simple webserver. It processes a request by:
1. Reading text from a TcpStream into a buffer
2. Parsing what is within that buffer
3. Generating a response and writing it to the TcpStream before closing it.

All text sent in the request will be stored as metadata while sending a response, as is, except for one exception. Percent encoding in the URL will be decoded.

### Responding to requests


### Testing and Testcases
Test cases are located in a batch script in `tests/curl-tests`.

If the program is compiled with `--features echo-test`, then any request made to `/api/echo` or `/api/echo/*`. The server will respond with all the information it could gather from the http request.

#### API Documentation
This is under construction
In order to interface with the API, a user must submit a valid token as a header in each HTTP request.
Command | Location | Result | Safe?
---|---|---|---
GET | /api/belgrade/documents | Retrieve a compressed file showing which documents exist on the server | Y
GET | /api/belgrade/documents/{document_id} | Retrieve a detailed summary of all documents with the specified Document ID. | Y
GET | /api/belgrade/tickets/{ticket_id} | Retrieve a detailed summary of all documents with the specified Ticket ID. |
GET | /api/belgrade/batches/{batch_id} | Retrieve a detailed summary of all documents with the specified document ID. |
POST | /api/belgrade/documents | Add a document to the database | Y

* Note: ?hashes=included can be added to the end of any URI in order

# Function Documentaion
handle_post:
Handles a POST Http request. This is where the PDF is received as a stream of bytes. The PDF will then be processed and placed into the appropriate file in the appropriate location with the appropriate metadata.

### Approximate speed testing
Buffers have now been implemented, and are available for various sizes. Though this means nothing, I was able to get ~250 requests per second on a single thread, where each request only had to be processed and a response had to be sent. I will try to get this number higher, I suspect a lot of it has to do with improper pointer usage (passing by value when I should be passing by reference). Though this performance isn't great, I am happy to have a good start.