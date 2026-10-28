# Now on to 2.0

Adding the next big feature set: Projects, Code Spaces, Live Searches, Action History, and Rule Sets.

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
        This implies being able to create, update, and load records for the new model types as mentioned above
1. The ability to apply your algorithms against a set of model records as defined by the load function.
    1. The end result will be either:
        1. An error
        1. A view of the results - all content view rules apply to this (tiling, etc.)

More functionality to come in the following features.

## Projects

Projects are a way to organize everything that you are doing.

## Live Searches, Action History, and Rule Sets

I want to be able to create take data sets, searches, or even individual rows of data, add them to a project (with the ability to create a new project).

I should have a code space. I can create code that hooks into the data in the should be able to 

I should be able to save searches - the query, filters, and inputs making up that search - as a Live Search, which can also be add to a project or stand alone with the rest of my live searches