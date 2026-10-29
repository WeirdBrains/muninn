# Now on to 2.0

Adding the next big feature set: Code Spaces, Projects, Reports, Journals, Authorship, Live Searches, Action History, Rule Sets, and more.

## Code Spaces

Code spaces allow us to create algorithms that work with the data we ingest. There should be:

1. Variables for all features (or attributes) in a model (or row) for current model types (or tables)
1. Functions to:
  1. Load records from imported data into a vector of model inputs
    1. We should be able to apply filters, grouping, and sorting - all by feature - to the load request
  1. Update records
    1. Update one model record by id
    1. Update many model records by an attribute filter match
  1. Create new records
    1. This can be useful as a result of calculations
  1. Create new model types with possible relationship to other model types
    1. This implies being able to create, update, and load records for the new model types as mentioned above
1. The ability to apply your algorithms against a set of model records as defined by the load function.
  1. The end result will be either:
    1. An error
    1. A view of the results - all content view rules apply to this (tiling, etc.)

More functionality to come in the following features.

## Projects

Projects are a way to organize everything that you are doing. There should be:

1. A projects page
  1. With a menu category in the top bar
  1. With a list of current projects
  1. With a button to create a new project
1. An add to project button on all:
  1. Search results
  1. Live searches
  1. Results
  1. Rule sets
  1. Journals
1. The ability to export a project:
  1. Data gets exported like a full export should with format choices:
    1. HTML
    1. JSON
    1. Latex
    1. Docx
    1. PDF
  1. Journals entries to be exported in a format chosen by the user:
    1. HTML
    1. JSON
    1. PDF
    1. Latex
    1. Docx
  1. Projects get exported with the full set of attachments, data, journals, etc. saved in them. They must be importable later.

## Reports

Reports are meant for humans to read and use. That's the goal of all this.

1. Reports will be exportable in the following formats:
    1. HMTL
    1. JSON
    1. CSV
    1. PDF
    1. Latex
    1. Docx
1. Reports are made of of various pieces of data:
    1. Model records
    1. Journal entries
    1. Project related data
    1. Authorship
    1. Output from code spaces

## Journals

A journal is a place for me to write what I'm working on, thoughts, frustrations, and just how your day is going, etc.

1. Journals can live:
    1. On their own
    1. In projects (one to many)
1. A journal can be exported and imported
1. They can contain images and be saved in a preferred format:
    1. Markdonw
    1. Latex
1. In projects1. A journal can be exported and imported
1. They can contain images and be saved in a preferred format:
    1. Markdonw
    1. Latex
1. Journal entries and the entire journal can be saved as the following formats:
    1. HTML
    1. JSON
    1. Latex
    1. PDF
    1. Docx
1. Journal entries and even an entire journal can be printed
1. Journals can be forked and saved into a project specific journal
1. Journal entries can be selected and added to other journals or a new one
1. Notes can be added to a journal globally, to specific entries, or even selected words on an entry
1. Highlights can be added to selected words on an entry
1. All notes, highlights, entries, and journals will be attributed to the author

## Authorship

Authorship allows us to tie our name and contacts info to the work that we've done.

1. An author's name, email, and other misc. details can be created
1. All these details can be applied by various features in the application

## Live Searches

Live searches are the means of storing your questions. This is not the results of a search, but the search itself.

1. Any search can be saved as a live search
1. Any live search can be fork and optimized
1. Live searches can be added to projects
1. Anything that is searchable can be pulled up by a live search
1. Live searches should be accessable from the main menu, through shortcuts, and in projects
1. They always act on data, not old results; hence, they always update results when data changes
1. Live searches stay with the database; no intentions are lost

## Action History

Action history allows us to see the steps that brought us to our current state, step back, and continue our journey.

1. Actions are save in the database
1. When you step back and do something different, a fork of the action history is created - all forks are saved
1. They live on the UI in a collapsable bar on the left

## Rule Sets

Rule sets define triggers that are processed when an action is done. An action could be a data import, an export, etc. Rule sets act as our if this then that for the data world.

1. Emulates if this/then that style of rules
1. They have fields and filters that meet the particular input/output actions we support
1. They are parsed and executed (if matching) everytime that an action is taken
1. They can live in a project, journal, live search, or on their own.
1. They are stored with the database so that they live beyond one device

## Decentralized Network

The decentralized network frees us from relying on any one system. Peers can carry our data, relays can broudcast our capabilities, live searches can reach across the network.

1. Uses libp2p for maximum support; globally and locally
1. Network becomes a main menu bar category with supporting items
1. Key exchanges are the mechanism for decrypting private data between different nodes
1. Key exchanges are always stored, done between nodes, and made restorable
1. Dark shards can be entrusted to or restored from other nodes after key exchanges

## Model Designer

The model designer allows you to create new shapes and forms for your data to be stored. It is your data playground, that shapes your database.

1. Model designs create, modify, or delete new or current tables for records
1. Model designs can be created, edited, or deleted
1. Model designs act aas versioned database migrations
1. They are built on top of rule sets
1. They can be used, created, modified, and deleted, in turn, by rule sets
1. Data designs must migrate with the data itself: if data is transferred, the model designs that are tied to it do as well, and all rule sets that created of modified them
1. This implies that every piece of data is versioned to conform to their model designs; that there is a relationship between a table and its model design - as well as all the data in the table
1. There will be an easy to use editor to design models
1. the editor will build and execute rule sets to fit the design

## Organizations

Organizations allow us to compartmentalize our work. They act as a publishing house, a gatekeeper, and a source of membership.

1. An organization can be created, modified, and deleted by the owner
1. It can have many roles (default: owner, admin, user, viewer)
1. Roles control access to an organizations resources and actions
1. It can have many memberships that each have a role (default: user)
1. Membership role defaults can be managed by the proper roled members (default: owner, admin)
1. An organization can be public or private (default: private)
1. They can have members or be open
1. All organizations that require membership (public or private) will need to do diffie-hellman key exchanges between applying members and the organization nodes
1. Potential members can apply for membership
1. Organization members with the proper roles (default: owner, admin) can approve or reject potential members
1. Organizations will have web hosting:
    1. Reports, journals, data, and record exports will be served from every organization node via https
    1. Short term certificates will be generated per node from letsencrypt
    1. Members, if configured that way, can gain access using their shared key to authorize them
    1. Organizations can select which artifacts that they want published
    !. Each organization node 
    1. The end result is gRPC, JSON, HTML and shared assets servable
1. Organizations can design templates to change the look and feel of their public web hosting
    1. All assets used for the template will be stored with the templates and made accessible via hosting
1. Organizations can user their hosting to execute requests that may retrieve or affect model records stored
    1. This can involve service plugins installed on the node
    1. Or rule sets exposed over gRPC
1. Organizations can have many projects
1. Projects, in turn, can have many members
1. Organizational roles and rules apply to projects and their membership
1. Any resource transferred to an organization by a member, is owned by the organization if the member is kicked out of the organization
    1. It can be archived, published, or left as is by the organization

## Reviews

Our work is better when we involve others. The reviews system allows us to share a link to others to our journal entries, projects, live searches, and even model records.

1. Review requests create a link that initiates a key exchange for that resource
1. The resource can be viewed, commented on, suggest changes, voted on, and highlighted by reviewers
1. Authors can accept or reject all changes, etc.
1. Reviews can be merged in to the model record changes as a versioned record

