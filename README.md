# This README is severely out of date

# The RMC Site
This is a site used internally by Redi-Mix Concrete in order to view documents and generate reports. In the future, it may be upgraded and used for customers as well.

Below is an overview of the site. For detailed documentation on either the back end or the front end, see the [backend README](backend/README.md) or [frontend README](./site/README.md) respectively

## Overview
This site is focused on utility and simplicity. It is barebones by design, in order to keep the programs that run on it smaller, simpler, and quicker than otherwise. The following is a brief overfiew of the programs that make this site work.

### NGINX
NGINX provides a small, fast, and simple webserver ideal for serving static content (webpages which are always the same for every user). It allows for easy SSL and https certification as well. 

All HTTP requests received by this site first go through an NGINX server. For simple requests (like a GET request for a static webpage), NGINX also serves the content. For more complex requests (like GET requests for variable or dynamically accessed content), NGINX will pass the request to a proxy server which is better suited to handling the request.

### CUSTOM RUST WEBSERVER
A custom rust webserver was built for this site. It acts as a proxy server which serves dynamic content (such as searches and reports). It does this by querying a database and generating its own HTML responses.

### PostgreSQL Database
A PostgreSQL database contains information about the documents stored on the server. It is not exposed to the internet, and only runs on localhost for security reasons. Therefore, it is only exposed (and can only be accessed by) programs running on the same computer.

### Administrative Programs
Any other programs that run on the server are used for administrative purposes (examples: ssh, chrontab, etc.).

### Notes
#### JavaScript
This site currently lacks any JavaScript. While it would be nice to be able to change the html on a page without reloading, this site is not complex enough to justify this right now. Less software = less issues.

#### Further Documentation
Detailed documentation on the back end and the front end are available in the backend and site folders respectively.

#### API Documentation
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
