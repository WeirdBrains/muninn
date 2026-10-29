# Now on to 2.0

Adding the next big feature set: Code Spaces, Projects, Reports, Journals, Authorship, Live Searches, Action History, and Rule Sets.

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

## Live Searches, Action History, and Rule Sets

I want to be able to create take data sets, searches, or even individual rows of data, add them to a project (with the ability to create a new project).

I should have a code space. I can create code that hooks into the data in the should be able to 

I should be able to save searches - the query, filters, and inputs making up that search - as a Live Search, which can also be add to a project or stand alone with the rest of my live searches
