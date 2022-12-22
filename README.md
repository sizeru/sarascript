# The RMC Site

This site is mainly handled by an nginx webserver, which is used to direct content to the right place. In the future, this webserver may be replaced by a custom solution.

Postgres is sometimes difficult to installa, but may be installed on Debian by default

When belgrade is accessed, nginx will redirect the request to a rust webserver

This site uses a custom webserver in the backend (using Rust) to serve up webpages and APIs. APIs follow the REST method of organizing content.

###### (This site has been stripped of any user data)

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